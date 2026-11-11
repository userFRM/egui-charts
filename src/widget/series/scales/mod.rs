//! Fitting and mapping the price scales shared by bar series and studies.

use super::{PriceSeries, SeriesLastValue};
use crate::model::{
    AutoscaleInfo, AutoscaleMargins, Bar, ChartType, Marker, MarkerOptions, MarkerPos,
    PriceLineSource, SeriesScale,
};
use crate::scales::{PriceMap, PriceScale, PriceScaleMode, PriceScaleOptions, PriceTransform};
use crate::studies::{Indicator, PlotType};
use crate::widget::{Chart, PaneId, SeriesId};
use chrono::{DateTime, Utc};
use egui::{Color32, Painter, Rect};

/// A price on a series axis, its tag colours, the map it is drawn on and its tag text.
type AxisValue = (f64, (Color32, Color32), PriceMap, String);

/// Anchor tags to their own axis column while retaining the pane's vertical bounds.
pub(in crate::widget) fn axis_label_rect(plot: Rect, axis: Rect) -> Rect {
    let x = if axis.center().x < plot.center().x {
        axis.right()..=plot.right()
    } else {
        plot.left()..=axis.left()
    };
    Rect::from_x_y_ranges(x, plot.y_range())
}

fn shared_prices(
    study: &dyn Indicator,
    range: std::ops::Range<usize>,
) -> impl Iterator<Item = f64> + '_ {
    let gradients = crate::studies::plots::gradient_prices(study, range.clone());
    (0..study.line_count())
        .filter(move |line| {
            let plot = study.plot_style(*line);
            plot.visible
                && !matches!(
                    plot.plot_type,
                    PlotType::BarColor | PlotType::BackgroundColor
                )
        })
        .flat_map(move |line| {
            let plot = study.plot_style(line);
            range
                .clone()
                .filter_map(move |bar| study.plot_value(line, bar))
                .filter(|value| value.is_finite())
                .chain(std::iter::once(0.0).filter(move |_| {
                    matches!(
                        plot.plot_type,
                        PlotType::Histogram | PlotType::Columns | PlotType::Area
                    )
                }))
        })
        .chain(gradients)
}

impl Chart {
    /// The pane's saved axis layout, or `None` for an unknown pane.
    pub fn pane_axis_options(&self, pane: PaneId) -> Option<crate::model::PaneAxisOptions> {
        self.pane_layout
            .panes
            .iter()
            .find(|item| item.id == pane)
            .map(|item| item.axes)
    }

    /// Replaces the pane's axis layout without changing scale assignments, options or ranges.
    /// Returns `false` for an unknown pane. Wrap host edits in [`Self::user_action`] for undo.
    pub fn set_pane_axis_options(
        &mut self,
        pane: PaneId,
        options: crate::model::PaneAxisOptions,
    ) -> bool {
        let Some(item) = self
            .pane_layout
            .panes
            .iter_mut()
            .find(|item| item.id == pane)
        else {
            return false;
        };
        item.axes = options;
        true
    }

    /// Places the pane's labelled axes beside each other at `side`, preserving their units.
    /// Hidden overlay scales stay unlabelled. Returns `false` for an unknown pane.
    pub fn place_pane_scales(&mut self, pane: PaneId, side: crate::model::AxisSide) -> bool {
        let Some(mut options) = self.pane_axis_options(pane) else {
            return false;
        };
        options.side = Some(side);
        self.set_pane_axis_options(pane, options)
    }

    /// Joins sources from the pane's displayed axes onto one scale at `side`. The main
    /// series' scale wins when displayed, otherwise the nearest named scale at that edge,
    /// then the first named scale. Its options and manual range are retained. Series keep
    /// their data and numeric formats; undo restores the previous assignments and ranges.
    /// Overlay assignments stay unlabelled. Returns `false` without changes for an unknown
    /// pane or a study that refuses the new assignment.
    pub fn merge_pane_scales(&mut self, pane: PaneId, side: crate::model::AxisSide) -> bool {
        use crate::studies::StudyScale;
        use crate::widget::PaneScale;
        if self.pane_axis_options(pane).is_none() {
            return false;
        }
        let mut keys = self.pane_scales(pane, side);
        keys.extend(self.pane_scales(
            pane,
            if side == crate::model::AxisSide::Left {
                crate::model::AxisSide::Right
            } else {
                crate::model::AxisSide::Left
            },
        ));
        let primary = PaneScale::Named(self.main_series_scale);
        let target = if pane == self.main_series_pane() && keys.contains(&primary) {
            self.main_series_scale
        } else {
            keys.iter()
                .find_map(|key| match key {
                    PaneScale::Named(key) => Some(*key),
                    _ => None,
                })
                .unwrap_or(SeriesScale::Right)
        };
        let members: Vec<_> = self
            .pane_series(pane)
            .into_iter()
            .filter(|id| {
                self.source_axis_key(pane, *id)
                    .is_some_and(|key| keys.contains(&key))
            })
            .collect();
        let studies: Vec<_> = members
            .iter()
            .filter_map(|id| self.indicators().find(|(member, _)| member == id))
            .filter(|(_, study)| {
                study.scale() != StudyScale::Shared(target)
                    && !(study.scale() == StudyScale::Main
                        && target == self.main_series_scale
                        && pane == self.main_series_pane())
            })
            .map(|(id, study)| (id, study.clone_box().set_scale(StudyScale::Shared(target))))
            .collect();
        if studies.iter().any(|(_, accepted)| !accepted) {
            return false;
        }
        self.pane_price_scale_mut(pane, target);
        for id in members {
            if self.series_scale(id).is_some_and(|key| key != target) {
                if self
                    .series_style(id)
                    .is_some_and(|style| style.display_format.is_none())
                {
                    let precision = self.series_price_scale(id).and_then(|scale| {
                        scale.map(Default::default(), 0.0, 1.0).price_precision()
                    });
                    if let Some(precision) = precision {
                        self.series_style_mut(id)
                            .expect("present series")
                            .display_format = Some(crate::model::SeriesDisplayFormat {
                            precision: precision.min(18) as u8,
                            min_move: 0.0,
                            ..Default::default()
                        });
                    }
                }
                self.set_series_scale(id, target);
            }
        }
        for (id, _) in studies {
            self.set_indicator_scale(id, StudyScale::Shared(target));
        }
        self.place_pane_scales(pane, side)
    }

    /// The labelled scales displayed on one side of a pane, nearest the plot first.
    /// Shared assignments appear once; independent studies have their own identities.
    /// Hidden overlays and, by default, axes with no visible sources are
    /// omitted. A visible source with no data retains its axis. Unknown panes return no axes.
    pub fn pane_scales(
        &self,
        pane: PaneId,
        side: crate::model::AxisSide,
    ) -> Vec<crate::widget::PaneScale> {
        self.pane_scale_keys(pane)
            .filter(|key| self.pane_scale_side(pane, *key) == Some(side))
            .collect()
    }

    pub(in crate::widget) fn pane_scale_count(
        &self,
        pane: PaneId,
        side: crate::model::AxisSide,
    ) -> usize {
        self.pane_scale_keys(pane)
            .filter(|key| self.pane_scale_side(pane, *key) == Some(side))
            .count()
    }

    /// The scale behind an axis of [`Self::pane_scales`]. `None` for an axis the pane does not
    /// display, an independent study, which maps itself, or a study assignment before the
    /// frame that creates its scale.
    pub fn pane_scale(&self, pane: PaneId, axis: crate::widget::PaneScale) -> Option<&PriceScale> {
        let crate::widget::PaneScale::Named(key) = axis else {
            return None;
        };
        if !self.pane_scale_keys(pane).any(|key| key == axis) {
            None
        } else if pane == PaneId::MAIN && key != self.main_series_scale {
            self.series_scales.get(&key)
        } else if pane == PaneId::MAIN
            || (pane == self.main_series_pane() && key == self.main_series_scale)
        {
            Some(self.pane_price_scale(pane, key))
        } else {
            self.pane_layout
                .panes
                .iter()
                .find(|item| item.id == pane)?
                .own_scale(key)
        }
    }

    /// Changes options or a manual range on the scale behind an axis of [`Self::pane_scales`].
    /// `None` for an axis the pane does not display or an independent study, which maps itself.
    pub fn pane_scale_mut(
        &mut self,
        pane: PaneId,
        axis: crate::widget::PaneScale,
    ) -> Option<&mut PriceScale> {
        let crate::widget::PaneScale::Named(key) = axis else {
            return None;
        };
        let present = self.pane_scale_keys(pane).any(|key| key == axis);
        present.then(|| self.pane_price_scale_mut(pane, key))
    }

    fn pane_scale_keys(&self, pane: PaneId) -> impl Iterator<Item = crate::widget::PaneScale> {
        use crate::widget::PaneScale;
        let Some(options) = self.pane_axis_options(pane) else {
            return None.into_iter().chain(Vec::new());
        };
        let main = pane == self.main_series_pane();
        let primary = if main {
            self.main_series_scale
        } else {
            SeriesScale::Right
        };
        let mut keys = Vec::new();
        let mut show_primary = !options.hide_unused && !matches!(primary, SeriesScale::Overlay(_));
        for id in self.pane_series_iter(pane) {
            if options.hide_unused && !self.is_series_visible(id) {
                continue;
            }
            let Some(key) = self.source_axis_key(pane, id) else {
                continue;
            };
            if key == PaneScale::Named(primary) && !matches!(primary, SeriesScale::Overlay(_)) {
                show_primary = true;
            } else if !matches!(key, PaneScale::Named(SeriesScale::Overlay(_)))
                && !keys.contains(&key)
            {
                keys.push(key);
            }
        }
        // The primary axis stays nearest the plot regardless of the series' draw order.
        show_primary
            .then_some(PaneScale::Named(primary))
            .into_iter()
            .chain(keys)
    }

    pub(in crate::widget) fn source_axis_key(
        &self,
        pane: PaneId,
        id: SeriesId,
    ) -> Option<crate::widget::PaneScale> {
        use crate::widget::PaneScale;
        if let Some(key) = self.series_scale(id) {
            return Some(PaneScale::Named(key));
        }
        let study = self.indicators().find(|(member, _)| *member == id)?.1;
        let main = pane == self.main_series_pane();
        match study.scale() {
            crate::studies::StudyScale::None => None,
            crate::studies::StudyScale::Shared(key) => Some(PaneScale::Named(key)),
            crate::studies::StudyScale::Independent
                if (main || pane == PaneId::MAIN) && !study.is_volume() =>
            {
                Some(PaneScale::Study(id))
            }
            _ if (main || pane == PaneId::MAIN) && study.is_volume() => None,
            _ => Some(PaneScale::Named(if main {
                self.main_series_scale
            } else {
                SeriesScale::Right
            })),
        }
    }

    pub(in crate::widget) fn pane_scale_side(
        &self,
        pane: PaneId,
        key: crate::widget::PaneScale,
    ) -> Option<crate::model::AxisSide> {
        use crate::model::AxisSide;
        use crate::widget::PaneScale;
        let side = self.pane_axis_options(pane)?.side;
        let primary = PaneScale::Named(if pane == self.main_series_pane() {
            self.main_series_scale
        } else {
            SeriesScale::Right
        });
        if (key == primary || matches!(key, PaneScale::Study(_)))
            && !self.config.show_left_axis
            && !self.config.show_right_axis
        {
            return None;
        }
        let right = if key == primary {
            self.config.show_right_axis
        } else {
            key == PaneScale::Named(SeriesScale::Right)
        };
        Some(side.unwrap_or(if right {
            AxisSide::Right
        } else {
            AxisSide::Left
        }))
    }

    /// Whether the main series' axis is placed at the left edge, even while it is hidden.
    pub(crate) fn main_axis_on_left(&self) -> bool {
        self.pane_scale_side(
            self.main_series_pane(),
            crate::widget::PaneScale::Named(self.main_series_scale),
        ) == Some(crate::model::AxisSide::Left)
    }

    pub(in crate::widget) fn pane_scale_axes(
        &self,
        pane: PaneId,
        panel: Rect,
        plot: Rect,
    ) -> impl Iterator<Item = (crate::widget::PaneScale, Rect)> + '_ {
        use crate::model::AxisSide;
        let reference = self
            .pane_layout
            .panes
            .iter()
            .find(|pane| pane.id == self.main_series_pane() && pane.rect.is_positive())
            .map_or(panel, |pane| pane.rect);
        let padding = self
            .config
            .padding
            .max(0.0)
            .min(reference.width().min(reference.height()).max(0.0) / 2.0);
        [AxisSide::Left, AxisSide::Right]
            .into_iter()
            .filter_map(move |side| {
                let count = self.pane_scale_count(pane, side);
                if count == 0 {
                    return None;
                }
                let available = match side {
                    AxisSide::Left => plot.left() - panel.left() - padding,
                    AxisSide::Right => panel.right() - plot.right() - padding,
                };
                // Layout has already reserved these columns. Measurements can change while
                // painting another pane, so use the allocated gutter for every pane this frame.
                let columns = self
                    .pane_layout
                    .panes
                    .iter()
                    .filter(|item| {
                        item.id == PaneId::MAIN
                            || item.id == self.main_series_pane()
                            || self.pane_shown(item)
                    })
                    .map(|item| {
                        if item.id == pane {
                            count
                        } else {
                            self.pane_scale_count(item.id, side)
                        }
                    })
                    .max()
                    .unwrap_or(0);
                let width = available.max(0.0) / columns.max(1) as f32;
                Some(
                    self.pane_scale_keys(pane)
                        .filter(move |key| self.pane_scale_side(pane, *key) == Some(side))
                        .enumerate()
                        .map(move |(at, key)| {
                            let offset = at as f32 * width;
                            let left = match side {
                                AxisSide::Left => plot.left() - offset - width,
                                AxisSide::Right => plot.right() + offset,
                            };
                            let right = if side == AxisSide::Right
                                && at + 1 == count
                                && panel.right() - (left + width) <= padding + 0.1
                            {
                                panel.right()
                            } else {
                                left + width
                            };
                            (key, Rect::from_x_y_ranges(left..=right, panel.y_range()))
                        }),
                )
            })
            .flatten()
    }

    pub(in crate::widget) fn primary_axis_side(&self) -> Option<crate::model::AxisSide> {
        let pane = self.main_series_pane();
        let key = crate::widget::PaneScale::Named(self.main_series_scale);
        if self.pane_scale_keys(pane).any(|axis| axis == key) {
            self.pane_scale_side(pane, key)
        } else {
            None
        }
    }

    /// The drawn pane at a menu click, including its axis gutters.
    #[cfg(feature = "ui")]
    pub(crate) fn pane_at_position(&self, pos: egui::Pos2) -> Option<PaneId> {
        self.pane_layout
            .panes
            .iter()
            .find(|pane| pane.rect.contains(pos))
            .map(|pane| pane.id)
    }

    #[cfg(feature = "ui")]
    pub(crate) fn axis_at_position(
        &self,
        pos: egui::Pos2,
    ) -> Option<(PaneId, crate::widget::PaneScale)> {
        let pane = self
            .pane_layout
            .panes
            .iter()
            .find(|pane| pane.rect.contains(pos))?;
        let plot =
            Rect::from_x_y_ranges(self.last_rendered_price_rect.x_range(), pane.rect.y_range());
        self.pane_scale_axes(pane.id, pane.rect, plot)
            .find(|(_, axis)| axis.contains(pos))
            .map(|(key, _)| (pane.id, key))
    }

    pub(in crate::widget) fn series_axis_keys(&self) -> Vec<SeriesScale> {
        self.pane_scale_keys(self.main_series_pane())
            .filter_map(|key| match key {
                crate::widget::PaneScale::Named(key) if key != self.main_series_scale => Some(key),
                _ => None,
            })
            .collect()
    }

    pub(in crate::widget) fn ensure_series_scale(&mut self, scale: SeriesScale) {
        if scale != self.main_series_scale {
            self.series_scales
                .entry(scale)
                .or_insert_with(|| PriceScale::new(PriceScaleOptions::default()));
        }
    }

    pub(in crate::widget) fn prune_series_scales(&mut self) {
        self.series_scales.retain(|scale, _| {
            self.price_series
                .iter()
                .any(|item| item.options.scale == *scale)
                || self
                    .studies
                    .indicators()
                    .iter()
                    .any(|study| study.scale() == crate::studies::StudyScale::Shared(*scale))
        });
    }

    pub(crate) fn pane_price_scale(&self, pane: PaneId, key: SeriesScale) -> &PriceScale {
        if pane == self.main_series_pane() && key == self.main_series_scale {
            self.state.price_scale()
        } else if pane == PaneId::MAIN && key != self.main_series_scale {
            &self.series_scales[&key]
        } else {
            let pane = self
                .pane_layout
                .panes
                .iter()
                .find(|item| item.id == pane)
                .expect("present pane");
            if pane.id == PaneId::MAIN {
                &pane.scale
            } else {
                pane.assigned_scale(key)
            }
        }
    }

    pub(crate) fn pane_price_scale_mut(
        &mut self,
        pane: PaneId,
        key: SeriesScale,
    ) -> &mut PriceScale {
        if pane == self.main_series_pane() && key == self.main_series_scale {
            self.state.price_scale_mut()
        } else if pane == PaneId::MAIN && key != self.main_series_scale {
            self.series_scales
                .entry(key)
                .or_insert_with(|| PriceScale::new(PriceScaleOptions::default()))
        } else {
            let pane = self
                .pane_layout
                .panes
                .iter_mut()
                .find(|item| item.id == pane)
                .expect("present pane");
            if pane.id == PaneId::MAIN {
                &mut pane.scale
            } else {
                pane.assigned_scale_mut(key)
            }
        }
    }

    pub(super) fn secondary_scale(&self, scale: SeriesScale) -> &PriceScale {
        self.pane_price_scale(self.main_series_pane(), scale)
    }

    /// The times of the first and the last bar slot in view, whitespace included.
    fn visible_times(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let logical = self.state.time_scale().visible_logical_range();
        let bars = self.state.shown();
        crate::model::time_at(bars, logical.left.floor() as f64)
            .zip(crate::model::time_at(bars, logical.right.ceil() as f64))
            .filter(|(first, last)| first <= last)
    }

    pub(in crate::widget) fn series_range(&self, item: &PriceSeries) -> std::ops::Range<usize> {
        match self.visible_times() {
            Some((first, last)) => {
                item.bars.partition_point(|bar| bar.time < first)
                    ..item.bars.partition_point(|bar| bar.time <= last)
            }
            None => 0..0,
        }
    }

    /// The bar whose close the series' price line and last value label show.
    pub(super) fn series_last_bar<'a>(&self, item: &'a PriceSeries) -> Option<&'a Bar> {
        self.series_bar_from(item, item.options.style.price_line_source)
    }

    fn series_bar_from<'a>(
        &self,
        item: &'a PriceSeries,
        source: PriceLineSource,
    ) -> Option<&'a Bar> {
        match source {
            PriceLineSource::LastBar => crate::model::last_with_value(&item.bars),
            PriceLineSource::LastVisible => {
                crate::model::last_with_value(&item.bars[self.series_range(item)])
            }
        }
    }

    /// The rising and falling body colours of the main series: Heikin-Ashi, hollow and volume
    /// candles keep their own, other chart types use the chart configuration.
    pub(in crate::widget) fn main_body_colors(&self) -> (Color32, Color32) {
        let styles = &self.chart_type_params.styles;
        match self.chart_type {
            ChartType::Heikin => (styles.heikin.body_up, styles.heikin.body_down),
            ChartType::HollowCandles => (styles.hollow.body_up, styles.hollow.body_down),
            ChartType::VolumeCandles => (
                styles.volume_candles.body_up,
                styles.volume_candles.body_down,
            ),
            _ => (self.config.bullish_color, self.config.bearish_color),
        }
    }

    /// The main series' last-value colour for the bar at `index` of `bars`: the price line
    /// colour, the bar's own colour, or its direction. Columns, hollow candles, and candles,
    /// volume candles and OHLC bars coloured by the previous close, take the direction their
    /// painters give the bar.
    pub(in crate::widget) fn main_value_color(&self, bars: &[Bar], index: usize) -> Color32 {
        let styles = &self.chart_type_params.styles;
        let (up, down) = match self.chart_type {
            ChartType::HighLow => {
                let color = styles.high_low.bar_color;
                (color, color)
            }
            _ => self.main_body_colors(),
        };
        let bar = &bars[index];
        let by_previous_close = match self.chart_type {
            ChartType::HollowCandles | ChartType::Columns => true,
            ChartType::Candles => styles.candles.color_by_previous_close,
            ChartType::VolumeCandles => styles.volume_candles.color_by_previous_close,
            ChartType::Bars => styles.bars.color_by_previous_close,
            _ => false,
        };
        let reference = if by_previous_close {
            crate::chart::rendering::candles::prev_close(bars, index)
        } else {
            bar.open
        };
        self.main_series_style
            .price_line_color
            .or_else(|| bar.colors.as_ref().and_then(|own| own.color))
            .unwrap_or(if bar.close >= reference { up } else { down })
    }

    /// The latest loaded or latest visible valued bar of a main or added bar series.
    /// Trailing whitespace is skipped; `source` is independent of the price-line setting.
    /// Hidden series and hidden value labels remain queryable.
    ///
    /// Uses the series' price scale, pane and formatter, as last drawn. Visible selection
    /// includes bars cut by a plot edge, as rendering does. On price-based main charts,
    /// loaded selection uses stored bars and visible selection uses displayed elements.
    /// Returns `None` for absent series, studies, a selection without values, or before the
    /// pane is drawn. A close the scale cannot place, such as zero or a negative price on a
    /// logarithmic scale, also returns `None`.
    pub fn series_last_value(
        &self,
        id: SeriesId,
        source: PriceLineSource,
    ) -> Option<SeriesLastValue> {
        let map = self.series_price_map(id)?;
        let (bar, color, index) = if id == SeriesId::MAIN {
            let (bars, start) = match source {
                PriceLineSource::LastBar => (self.bars(id), 0),
                PriceLineSource::LastVisible => {
                    let (start, end) = self.state.visible_range();
                    (&self.state.shown()[..end], start)
                }
            };
            let at = start + bars[start..].iter().rposition(|bar| !bar.is_whitespace())?;
            let bar = &bars[at];
            let index = match source {
                PriceLineSource::LastBar => self.time_to_index(bar.time)?,
                PriceLineSource::LastVisible => at as f64,
            };
            (bar, self.main_value_color(bars, at), index)
        } else {
            let item = self.price_series.iter().find(|item| item.id == id)?;
            let bar = self.series_bar_from(item, source)?;
            (bar, item.value_color(bar), self.time_to_index(bar.time)?)
        };
        let coordinate = map.price_to_y(bar.close);
        if !coordinate.is_finite() || !map.transform().to_scale(bar.close).is_finite() {
            return None;
        }
        Some(SeriesLastValue {
            value: bar.close,
            index,
            coordinate,
            color,
            text: self.format_series_price(id, bar.close)?,
        })
    }

    /// The prices of price-anchored markers of `markers` and `more` in view, when `options` fit them.
    pub(in crate::widget) fn marker_prices<'a>(
        &'a self,
        markers: &'a crate::widget::marks::TimeOrdered<Marker>,
        more: &'a [Marker],
        options: MarkerOptions,
    ) -> impl Iterator<Item = f64> + 'a {
        let window = self.visible_times().filter(|_| options.auto_scale);
        window
            .into_iter()
            .flat_map(move |(first, last)| {
                markers
                    .within(Some(first), Some(last))
                    .into_iter()
                    .map(move |index| &markers[index])
                    .chain(
                        more.iter()
                            .filter(move |marker| (first..=last).contains(&marker.time)),
                    )
            })
            .filter(|marker| {
                marker.size.is_finite() && marker.size > 0.0 && self.marker_in_view(marker)
            })
            .filter(|marker| {
                matches!(
                    marker.position,
                    MarkerPos::AtPriceTop | MarkerPos::AtPriceBottom | MarkerPos::AtPriceMiddle
                )
            })
            .filter_map(|marker| marker.price)
    }

    /// Whether the marker's centre is in the horizontal plot, using the renderer's mapping.
    fn marker_in_view(&self, marker: &Marker) -> bool {
        let time = self.state.time_scale();
        crate::model::index_at(self.state.shown(), marker.time).is_some_and(|index| {
            let x = time.idx_to_coord_precise(index, 0.0, time.width());
            (0.0..=time.width()).contains(&x)
        })
    }

    /// Price anchors in scale units, with the shape and text's screen-space extents.
    pub(in crate::widget) fn series_marker_bounds(
        &self,
        painter: &Painter,
        id: SeriesId,
        map: PriceMap,
    ) -> Vec<(f64, f32, f32)> {
        let (marks, source, options, bars) = if id == SeriesId::MAIN {
            if !self.main_series_visible {
                return Vec::new();
            }
            (
                &self.marks,
                self.mark_source
                    .as_ref()
                    .map_or(&[][..], |s| &s.page.bar_marks),
                self.marker_options,
                self.state.shown(),
            )
        } else {
            let Some(item) = self
                .price_series
                .iter()
                .find(|item| item.id == id && item.visible)
            else {
                return Vec::new();
            };
            (&item.markers, &[][..], item.marker_options, &item.bars[..])
        };
        if !options.auto_scale || bars.is_empty() {
            return Vec::new();
        }
        let rect = Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(self.state.time_scale().width(), 1.0),
        );
        let coords = self.get_chart_mapping().with_rect(rect);
        let (from, to) = crate::widget::marks::window(self.state.shown(), &coords, rect);
        let markers = marks
            .within(from, to)
            .into_iter()
            .map(|at| &marks[at])
            .chain(source);
        let bounds = |marker: &Marker| {
            if !self.marker_in_view(marker) {
                return None;
            }
            let at = if id == SeriesId::MAIN {
                crate::model::index_at(bars, marker.time)?
                    .round()
                    .clamp(0.0, (bars.len() - 1) as f64) as usize
            } else {
                bars.partition_point(|bar| bar.time <= marker.time)
                    .saturating_sub(1)
            };
            let (price, top, bottom) =
                crate::chart::rendering::marker_bounds(painter, marker, &bars[at], &map)?;
            let value = map.transform().to_scale(price);
            (value.is_finite() && top.is_finite() && bottom.is_finite())
                .then_some((value, top, bottom))
        };
        if id == SeriesId::MAIN && self.chart_type.is_time_independent() {
            crate::chart::rendering::candles::price_based::markers(
                markers.enumerate(),
                &self.state,
                self.chart_type,
                &self.chart_type_params,
                self.older_bars_drawn,
            )
            .iter()
            .filter_map(|(_, marker)| bounds(marker))
            .collect()
        } else {
            markers.filter_map(bounds).collect()
        }
    }

    /// Fits provider margins and marker extents together in screen space. Without provider
    /// margins, or in a pane too short for them, retains the ordinary marker fit. Manual ranges
    /// keep their range.
    pub(in crate::widget) fn fit_marker_bounds(
        scale: &mut PriceScale,
        rect: Rect,
        bounds: impl IntoIterator<Item = (f64, f32, f32)>,
        margins: AutoscaleMargins,
    ) {
        if scale.is_manual() || rect.height() <= 0.0 {
            return;
        }
        let bounds: Vec<_> = bounds.into_iter().collect();
        if margins != AutoscaleMargins::default() {
            let options = scale.options().scale_margins;
            let (mut top, mut bottom) =
                (options.top.clamp(0.0, 1.0), options.bottom.clamp(0.0, 1.0));
            if top + bottom >= 1.0 {
                (top, bottom) = (0.0, 0.0);
            }
            let range = scale.range();
            let span = range.max - range.min;
            let (mut low, mut high) = (range.min + span * bottom, range.max - span * top);
            if scale.options().invert_scale {
                std::mem::swap(&mut top, &mut bottom);
            }
            let height = f64::from(rect.height());
            let (mut top, mut bottom) = (
                top * height + f64::from(margins.top),
                bottom * height + f64::from(margins.bottom),
            );
            for &(anchor, above, below) in &bounds {
                low = low.min(anchor);
                high = high.max(anchor);
                top = top.max(f64::from(above));
                bottom = bottom.max(f64::from(below));
            }
            if top + bottom < height {
                if low == high {
                    let half = scale.min_span() * 0.5;
                    low -= half;
                    high += half;
                }
                // Fit once with both requests, so fitting one edge cannot consume the other's
                // space.
                if scale.options().invert_scale {
                    std::mem::swap(&mut top, &mut bottom);
                }
                scale.options_mut().scale_margins = crate::scales::PriceScaleMargins {
                    top: top / height,
                    bottom: bottom / height,
                };
                scale.autoscale(low, high);
                scale.options_mut().scale_margins = options;
                return;
            }
        }
        let map = scale.map(scale.transform(), rect.top(), rect.bottom());
        let range = scale.range();
        let (mut low, mut high) = (range.min, range.max);
        let (mut top, mut bottom) = (0.0_f32, 0.0_f32);
        let mut clipped = false;
        for (anchor, above, below) in bounds {
            let y = map.scale_to_y(anchor);
            clipped |= y - above < rect.top() || y + below > rect.bottom();
            low = low.min(anchor);
            high = high.max(anchor);
            top = top.max(above);
            bottom = bottom.max(below);
        }
        let height = rect.height();
        if !clipped || top + bottom >= height {
            return;
        }
        if low == high {
            let half_span = scale.min_span() * 0.5;
            low -= half_span;
            high += half_span;
        }
        let span = (high - low) / f64::from(1.0 - (top + bottom) / height);
        let (lower, upper) = if scale.options().invert_scale {
            (top, bottom)
        } else {
            (bottom, top)
        };
        let margins = scale.options().scale_margins;
        // The existing range already includes the host's margins.
        scale.options_mut().scale_margins = crate::scales::PriceScaleMargins {
            top: 0.0,
            bottom: 0.0,
        };
        scale.autoscale(
            low - span * f64::from(lower / height),
            high + span * f64::from(upper / height),
        );
        scale.options_mut().scale_margins = margins;
    }

    /// Fits pixel contributions after every series in the main pane has fitted its prices.
    pub(in crate::widget) fn fit_marker_scales(
        &mut self,
        painter: &Painter,
        rect: Rect,
        margins: &std::collections::HashMap<SeriesScale, AutoscaleMargins>,
    ) {
        let map = self.state.price_scale().map(
            self.state.price_scale().transform(),
            rect.top(),
            rect.bottom(),
        );
        let mut main = self.series_marker_bounds(painter, SeriesId::MAIN, map);
        let mut other = std::collections::HashMap::<SeriesScale, Vec<_>>::new();
        for item in self.main_pane_series() {
            let bounds = self.series_marker_bounds(painter, item.id, self.series_map(item, rect));
            if item.options.scale == self.main_series_scale {
                main.extend(bounds);
            } else {
                other.entry(item.options.scale).or_default().extend(bounds);
            }
        }
        for key in margins.keys().filter(|key| **key != self.main_series_scale) {
            other.entry(*key).or_default();
        }
        let main_margins = margins
            .get(&self.main_series_scale)
            .copied()
            .unwrap_or_default();
        Self::fit_marker_bounds(self.state.price_scale_mut(), rect, main, main_margins);
        for (key, bounds) in other {
            let scale = self.pane_price_scale_mut(self.main_series_pane(), key);
            Self::fit_marker_bounds(
                scale,
                rect,
                bounds,
                margins.get(&key).copied().unwrap_or_default(),
            );
        }
    }

    fn series_transform(&self, item: &PriceSeries) -> PriceTransform {
        let mode = self
            .series_price_scale(item.id)
            .expect("present series")
            .options()
            .mode;
        let range = self.series_range(item);
        let base = item.bars[range]
            .iter()
            .find(|bar| !bar.is_whitespace())
            .map_or(f64::NAN, |bar| bar.close);
        PriceTransform::new(mode, base)
    }

    /// The visible host series drawn on the price pane.
    pub(in crate::widget) fn main_pane_series(
        &self,
    ) -> impl DoubleEndedIterator<Item = &PriceSeries> {
        self.price_series.iter().filter(|item| {
            item.visible && self.series_pane(item.id) == Some(self.main_series_pane())
        })
    }

    /// The close of `item` at the time of the shown bar `bar`, if it has a bar then.
    pub(in crate::widget) fn series_close_at(&self, item: &PriceSeries, bar: usize) -> Option<f64> {
        let time = self.state.shown().get(bar)?.time;
        item.bars
            .binary_search_by_key(&time, |bar| bar.time)
            .ok()
            .map(|at| &item.bars[at])
            .filter(|bar| !bar.is_whitespace())
            .map(|bar| bar.close)
    }

    /// The contribution of `item` in view: its rendered prices and columns' base, or the
    /// range and pixel margins its autoscale provider supplies.
    pub(in crate::widget) fn series_fit(&self, item: &PriceSeries) -> Option<AutoscaleInfo> {
        let heikin_ashi = (item.options.chart_type == ChartType::Heikin)
            .then(|| item.heikin_ashi.bars(&item.bars, item.revision));
        let bars = &heikin_ashi.as_deref().unwrap_or(&item.bars)[self.series_range(item)];
        let logarithmic = self
            .series_price_scale(item.id)
            .is_some_and(|scale| scale.options().mode == PriceScaleMode::Logarithmic);
        let base = item
            .options
            .columns_base
            .filter(|_| item.options.chart_type == ChartType::Columns && !bars.is_empty());
        let default = bars
            .iter()
            .flat_map(|bar| item.extremes(bar))
            .chain(
                bars.iter()
                    .flat_map(|bar| [bar.open, bar.close])
                    .filter(|_| logarithmic && item.options.chart_type.uses_ohlc()),
            )
            .chain(base)
            .filter(|value| value.is_finite() && (!logarithmic || *value > 0.0))
            .fold(None, |range: Option<(f64, f64)>, value| {
                Some(range.map_or((value, value), |(min, max)| {
                    (min.min(value), max.max(value))
                }))
            });
        match &item.options.style.autoscale {
            Some(provider) => provider.info(bars, default),
            None => default.map(Into::into),
        }
    }

    fn shared_study_transform(
        &self,
        id: SeriesId,
        study: &dyn Indicator,
        key: SeriesScale,
    ) -> PriceTransform {
        let (start, end) = self.state.visible_range();
        let base = (0..study.line_count())
            .filter(|line| study.plot_style(*line).visible)
            .find_map(|line| {
                (start..end).find_map(|bar| {
                    study
                        .plot_value(line, bar)
                        .filter(|value| value.is_finite() && *value != 0.0)
                })
            })
            .unwrap_or(f64::NAN);
        let scale = self.pane_price_scale(self.series_pane(id).expect("present study"), key);
        PriceTransform::new(scale.options().mode, base)
    }

    /// Fits each assignment in a secondary pane without borrowing another pane's scale.
    pub(in crate::widget) fn fit_pane_scales(
        &mut self,
        at: usize,
        painter: &Painter,
        visible: std::ops::Range<usize>,
    ) {
        let pane = self.pane_layout.panes[at].id;
        let members = self.pane_series(pane);
        let mut keys = vec![SeriesScale::Right];
        for id in &members {
            let key = self.series_scale(*id).or_else(|| {
                self.indicators()
                    .find(|(study, _)| study == id)
                    .and_then(|(_, study)| match study.scale() {
                        crate::studies::StudyScale::Shared(key) => Some(key),
                        _ => None,
                    })
            });
            if let Some(key) = key
                && !keys.contains(&key)
            {
                keys.push(key);
            }
        }
        for key in keys {
            self.pane_price_scale_mut(pane, key);
            let mut bounds = (f64::INFINITY, f64::NEG_INFINITY);
            let mut margins = AutoscaleMargins::default();
            let mut reference = None;
            let mode = self.pane_price_scale(pane, key).options().mode;
            let mut include = |bounds: &mut (f64, f64), value: f64, transform: PriceTransform| {
                if mode == PriceScaleMode::Logarithmic && value <= 0.0 {
                    return;
                }
                let value = transform.to_scale(value);
                if value.is_finite() {
                    reference.get_or_insert(transform);
                    bounds.0 = bounds.0.min(value);
                    bounds.1 = bounds.1.max(value);
                }
            };
            for item in self.price_series.iter().filter(|item| {
                members.contains(&item.id) && item.visible && item.options.scale == key
            }) {
                let transform = self.series_transform(item);
                for value in self
                    .series_fit(item)
                    .into_iter()
                    .chain(self.primitive_autoscale(item.id))
                    .flat_map(|info| {
                        margins.merge(info.margins);
                        [info.price_range.0, info.price_range.1]
                    })
                    .chain(self.marker_prices(&item.markers, &[], item.marker_options))
                {
                    include(&mut bounds, value, transform);
                }
            }
            if bounds.0.is_finite() && mode != PriceScaleMode::Logarithmic {
                let padding = crate::scales::auto_scale_span(bounds.0, bounds.1) * 0.1;
                bounds.0 -= padding;
                bounds.1 += padding;
            }
            for (&id, study) in self
                .study_ids
                .iter()
                .zip(self.rendered_studies().indicators())
            {
                let assignment = match study.scale() {
                    crate::studies::StudyScale::Shared(key) => key,
                    _ => SeriesScale::Right,
                };
                if !members.contains(&id) || !study.is_visible() || assignment != key {
                    continue;
                }
                // The initial pane's independent overlays retain their own mapping.
                if pane == PaneId::MAIN
                    && (study.scale() == crate::studies::StudyScale::Independent
                        || (study.is_volume()
                            && !matches!(study.scale(), crate::studies::StudyScale::Shared(_))))
                {
                    continue;
                }
                let transform = self.shared_study_transform(id, study.as_ref(), key);
                if mode == PriceScaleMode::Logarithmic {
                    for value in shared_prices(study.as_ref(), visible.clone()) {
                        include(&mut bounds, value, transform);
                    }
                } else if !self.is_custom_data(id)
                    && let Some(range) = crate::widget::IndicatorPane::with_config(
                        crate::widget::IndicatorPaneConfig::from_study(study.as_ref()),
                    )
                    .value_range(study.as_ref(), &visible)
                {
                    for value in [range.0, range.1] {
                        include(&mut bounds, value, transform);
                    }
                }
                for info in self.primitive_autoscale(id) {
                    margins.merge(info.margins);
                    for value in [info.price_range.0, info.price_range.1] {
                        include(&mut bounds, value, transform);
                    }
                }
            }
            let Some(mut transform) = reference else {
                continue;
            };
            // Raw and logarithmic ranges need no reference, including during study warm-up.
            if !transform.base.is_finite() {
                transform.base = 1.0;
            }
            let scale = self.pane_price_scale_mut(pane, key);
            let manual = (scale.is_manual() && scale.transform().mode == mode)
                .then(|| (scale.range(), scale.bar_ratio()));
            scale.rebase(transform);
            if let Some((range, ratio)) = manual {
                scale.set_range(range.min, range.max);
                // Restoring the range is not a zoom, which would rescale the ratio.
                scale.set_bar_ratio(ratio);
            }
            scale.autoscale(bounds.0, bounds.1);
            let rect = self.pane_layout.panes[at].rect;
            let markers: Vec<_> = self
                .price_series
                .iter()
                .filter(|item| {
                    members.contains(&item.id) && item.visible && item.options.scale == key
                })
                .flat_map(|item| {
                    self.series_marker_bounds(painter, item.id, self.series_map(item, rect))
                })
                .collect();
            let bar_spacing = self.state.time_scale().bar_spacing();
            let scale = self.pane_price_scale_mut(pane, key);
            Self::fit_marker_bounds(scale, rect, markers, margins);
            scale.update_bar_ratio(bar_spacing, rect.height());
        }
    }

    /// Applies the locked ratios of the scales beside the main series' scale in its pane.
    pub(in crate::widget) fn update_secondary_bar_ratios(&mut self, bar_spacing: f32, height: f32) {
        let pane = self.main_series_pane();
        if pane == PaneId::MAIN {
            for scale in self.series_scales.values_mut() {
                scale.update_bar_ratio(bar_spacing, height);
            }
        } else if let Some(pane) = self
            .pane_layout
            .panes
            .iter_mut()
            .find(|item| item.id == pane)
        {
            pane.scale.update_bar_ratio(bar_spacing, height);
            for (_, scale) in &mut pane.scales {
                scale.update_bar_ratio(bar_spacing, height);
            }
        }
    }

    /// Fits independent ranges in the main series' pane and returns its primary values.
    pub(in crate::widget) fn fit_secondary_series(
        &mut self,
    ) -> (
        Vec<f64>,
        std::collections::HashMap<SeriesScale, AutoscaleMargins>,
    ) {
        let pane = self.main_series_pane();
        let keys: Vec<_> = self
            .studies
            .indicators()
            .iter()
            .filter_map(|study| match study.scale() {
                crate::studies::StudyScale::Shared(key) => Some(key),
                _ => None,
            })
            .collect();
        for key in keys {
            self.ensure_series_scale(key);
        }
        let mut values = std::collections::HashMap::<
            SeriesScale,
            (PriceTransform, f64, f64, AutoscaleMargins),
        >::new();
        for item in self.main_pane_series() {
            let transform = self.series_transform(item);
            let key = item.options.scale;
            let entry = values.entry(key).or_insert((
                transform,
                f64::INFINITY,
                f64::NEG_INFINITY,
                AutoscaleMargins::default(),
            ));
            let markers = self.marker_prices(&item.markers, &[], item.marker_options);
            for value in self
                .series_fit(item)
                .into_iter()
                .chain(self.primitive_autoscale(item.id))
                .flat_map(|info| {
                    entry.3.merge(info.margins);
                    [info.price_range.0, info.price_range.1]
                })
                .chain(markers)
            {
                let value = transform.to_scale(value);
                if value.is_finite() {
                    if !entry.1.is_finite() {
                        entry.0 = transform;
                    }
                    entry.1 = entry.1.min(value);
                    entry.2 = entry.2.max(value);
                }
            }
        }
        let (start, end) = self.state.visible_range();
        for ((&id, study), &autoscale) in self
            .study_ids
            .iter()
            .zip(self.rendered_studies().indicators())
            .zip(&self.study_autoscale)
        {
            let crate::studies::StudyScale::Shared(key) = study.scale() else {
                continue;
            };
            if !study.is_visible()
                || self.series_pane(id) != Some(pane)
                || (key == self.main_series_scale
                    && (!autoscale || self.config.scale_price_chart_only))
            {
                continue;
            }
            let transform = self.shared_study_transform(id, study.as_ref(), key);
            let entry = values.entry(key).or_insert((
                transform,
                f64::INFINITY,
                f64::NEG_INFINITY,
                AutoscaleMargins::default(),
            ));
            let primitive_prices = self.primitive_autoscale(id).flat_map(|info| {
                entry.3.merge(info.margins);
                [info.price_range.0, info.price_range.1]
            });
            for value in shared_prices(study.as_ref(), start..end).chain(primitive_prices) {
                if transform.mode == PriceScaleMode::Logarithmic && value <= 0.0 {
                    continue;
                }
                let value = transform.to_scale(value);
                if value.is_finite() {
                    entry.1 = entry.1.min(value);
                    entry.2 = entry.2.max(value);
                }
            }
        }
        let mut main = Vec::new();
        let mut pixel_margins = std::collections::HashMap::new();
        for (key, (transform, min, max, margins)) in values {
            // The main series also fits its scale, so the frame decides on those margins.
            if margins != AutoscaleMargins::default()
                && (min.is_finite() || key == self.main_series_scale)
            {
                pixel_margins.insert(key, margins);
            }
            if !min.is_finite() {
                continue;
            }
            if key == self.main_series_scale {
                main.extend([min, max]);
            } else {
                let scale = self.pane_price_scale_mut(pane, key);
                // Membership and scrolling change references, not a manual range's scale units.
                let manual = (scale.is_manual() && scale.transform().mode == transform.mode)
                    .then(|| (scale.range(), scale.bar_ratio()));
                scale.rebase(transform);
                if let Some((range, ratio)) = manual {
                    scale.set_range(range.min, range.max);
                    // Restoring the range is not a zoom, which would rescale the ratio.
                    scale.set_bar_ratio(ratio);
                }
                scale.autoscale(min, max);
            }
        }
        (main, pixel_margins)
    }

    /// Supplies each study the mapping the chart owns: that of its shared scale, or for a study
    /// in the price pane on its own scale or no scale, its fit with the prices of its primitives.
    pub(in crate::widget) fn update_shared_study_maps(&mut self, rect: Rect) {
        let maps: Vec<_> = self
            .study_ids
            .iter()
            .zip(self.rendered_studies().indicators())
            .map(|(&id, study)| {
                let pane = self.series_pane(id)?;
                let key = match study.scale() {
                    crate::studies::StudyScale::Shared(key) => key,
                    crate::studies::StudyScale::Independent | crate::studies::StudyScale::None
                        if (pane == self.main_series_pane() || pane == PaneId::MAIN)
                            && !study.is_volume() =>
                    {
                        let prices = self
                            .primitive_autoscale(id)
                            .flat_map(|info| [info.price_range.0, info.price_range.1])
                            .filter(|price| price.is_finite())
                            .map(|price| (price, price))
                            .reduce(|(low, high), (lower, higher)| {
                                (low.min(lower), high.max(higher))
                            })?;
                        let coords = self.get_chart_mapping().with_rect(rect);
                        return Some(
                            crate::chart::rendering::fit_study_mapping(
                                study.as_ref(),
                                &coords,
                                Some(prices),
                            )
                            .price,
                        );
                    }
                    _ => return None,
                };
                if pane == self.main_series_pane() {
                    return Some(self.secondary_scale(key).map(
                        self.shared_study_transform(id, study.as_ref(), key),
                        rect.top(),
                        rect.bottom(),
                    ));
                }
                let pane = self.pane_layout.panes.iter().find(|item| item.id == pane)?;
                Some(self.pane_price_scale(pane.id, key).map(
                    self.shared_study_transform(id, study.as_ref(), key),
                    0.0,
                    1.0,
                ))
            })
            .collect();
        for (at, map) in maps.into_iter().enumerate() {
            self.studies.indicators_mut()[at].set_scale_mapping(map);
            if let Some(study) = self.element_studies.studies.indicators_mut().get_mut(at) {
                study.set_scale_mapping(map);
            }
        }
    }

    pub(in crate::widget) fn series_map(&self, item: &PriceSeries, rect: Rect) -> PriceMap {
        self.series_display_map(
            item.id,
            self.series_price_scale(item.id)
                .expect("present series")
                .map(self.series_transform(item), rect.top(), rect.bottom()),
        )
        .with_thousands_sep(self.config.thousands_sep)
    }

    pub(in crate::widget) fn secondary_axis_values(
        &self,
        key: SeriesScale,
        rect: Rect,
    ) -> impl Iterator<Item = AxisValue> + '_ {
        self.main_pane_series()
            .filter(move |item| item.options.scale == key)
            .flat_map(move |item| {
                let map = self.series_map(item, rect);
                self.series_axis_values(item)
                    .filter(move |(value, ..)| {
                        map.range().contains(map.transform().to_scale(*value))
                    })
                    .map(move |(value, colors, last)| {
                        (
                            value,
                            colors,
                            map,
                            self.series_tag_label(item, &map, value, last),
                        )
                    })
            })
    }

    /// The text of a value tag of `item` on `map`. Hosts' formatters write it; otherwise it has
    /// the scale's precision, led by the series name for the last value when the axis shows
    /// names.
    pub(in crate::widget) fn series_tag_label(
        &self,
        item: &PriceSeries,
        map: &PriceMap,
        value: f64,
        last_value: bool,
    ) -> String {
        let transform = map.transform();
        crate::chart::rendering::axes::named_label(
            last_value && self.config.price_axis.show_symbol_name,
            &item.options.name,
            map.label(
                transform.to_scale(value),
                crate::chart::rendering::axes::tag_decimals(map, map.price_to_y(value)),
                self.series_label_formatter(item.id, transform.mode)
                    .as_deref(),
            ),
        )
    }

    /// The price-line labels and the last close that `item` shows on its price axis, each with
    /// its tag colours and whether it is the last close.
    pub(in crate::widget) fn series_axis_values<'a>(
        &'a self,
        item: &'a PriceSeries,
    ) -> impl Iterator<Item = (f64, (Color32, Color32), bool)> + 'a {
        item.lines
            .iter()
            .filter(|(_, line)| line.axis_label)
            .map(|(_, line)| {
                (
                    line.price,
                    crate::chart::rendering::axes::axis_label_colors(line),
                    false,
                )
            })
            .chain(
                self.series_last_bar(item)
                    .filter(|_| {
                        self.config.show_symbol_last_val && item.options.style.last_value_visible
                    })
                    .map(|bar| {
                        let color = item.value_color(bar);
                        let text = crate::chart::rendering::axes::tag_text_color(color);
                        (bar.close, (text, color), true)
                    }),
            )
    }
}

#[cfg(test)]
mod tests;
