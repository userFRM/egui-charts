//! Fitting and mapping the price scales shared by bar series and studies.

use super::PriceSeries;
use crate::model::{
    Bar, ChartType, Marker, MarkerOptions, MarkerPos, PriceLineSource, SeriesScale,
};
use crate::scales::{PriceMap, PriceScale, PriceScaleMode, PriceScaleOptions, PriceTransform};
use crate::studies::{Indicator, PlotType};
use crate::widget::{Chart, PaneId};
use chrono::{DateTime, Utc};
use egui::{Color32, Rect};

/// A price on a series axis, its tag colours, the map it is drawn on and its tag text.
type AxisValue = (f64, (Color32, Color32), PriceMap, String);

fn shared_prices(
    study: &dyn Indicator,
    range: std::ops::Range<usize>,
) -> impl Iterator<Item = f64> + '_ {
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
}

impl Chart {
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

    pub(super) fn secondary_scale(&self, scale: SeriesScale) -> &PriceScale {
        if scale == self.main_series_scale {
            self.state.price_scale()
        } else {
            &self.series_scales[&scale]
        }
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
        match item.options.style.price_line_source {
            PriceLineSource::LastBar => crate::model::last_with_value(&item.bars),
            PriceLineSource::LastVisible => {
                crate::model::last_with_value(&item.bars[self.series_range(item)])
            }
        }
    }

    /// The prices of price-anchored markers of `markers` and `more` in view, when `options` fit them.
    pub(in crate::widget) fn marker_prices<'a>(
        &self,
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
                matches!(
                    marker.position,
                    MarkerPos::AtPriceTop | MarkerPos::AtPriceBottom | MarkerPos::AtPriceMiddle
                )
            })
            .filter_map(|marker| marker.price)
    }

    fn series_transform(&self, item: &PriceSeries) -> PriceTransform {
        let mode = self.secondary_scale(item.options.scale).options().mode;
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
        self.price_series
            .iter()
            .filter(|item| item.visible && !self.in_pane(item.id))
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

    /// The range the bars of `item` in view contribute to the fit of its scale: the prices its
    /// chart type draws and the columns' base, or what its autoscale provider makes of them.
    pub(in crate::widget) fn series_fit(&self, item: &PriceSeries) -> Option<(f64, f64)> {
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
            Some(provider) => provider.range(bars, default),
            None => default,
        }
    }

    fn shared_study_transform(&self, study: &dyn Indicator, key: SeriesScale) -> PriceTransform {
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
        PriceTransform::new(self.secondary_scale(key).options().mode, base)
    }

    /// Fits every independent range and returns the secondary values sharing the main range.
    pub(in crate::widget) fn fit_secondary_series(&mut self) -> Vec<f64> {
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
        let mut values =
            std::collections::HashMap::<SeriesScale, (PriceTransform, f64, f64)>::new();
        for item in self.main_pane_series() {
            let transform = self.series_transform(item);
            let entry = values.entry(item.options.scale).or_insert((
                transform,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ));
            let markers = self.marker_prices(&item.markers, &[], item.marker_options);
            for value in self
                .series_fit(item)
                .into_iter()
                .flat_map(|(min, max)| [min, max])
                .chain(markers)
                .chain(self.primitive_prices(item.id))
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
                || self.in_pane(id)
                || (key == self.main_series_scale
                    && (!autoscale || self.config.scale_price_chart_only))
            {
                continue;
            }
            let transform = self.shared_study_transform(study.as_ref(), key);
            let entry = values
                .entry(key)
                .or_insert((transform, f64::INFINITY, f64::NEG_INFINITY));
            for value in shared_prices(study.as_ref(), start..end).chain(self.primitive_prices(id))
            {
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
        for (key, (transform, min, max)) in values {
            if !min.is_finite() {
                continue;
            }
            if key == self.main_series_scale {
                main.extend([min, max]);
            } else if let Some(scale) = self.series_scales.get_mut(&key) {
                // Membership and scrolling change references, not a manual range's scale units.
                let manual = (scale.is_manual() && scale.transform().mode == transform.mode)
                    .then(|| scale.range());
                scale.rebase(transform);
                if let Some(range) = manual {
                    scale.set_range(range.min, range.max);
                }
                scale.autoscale(min, max);
            }
        }
        main
    }

    pub(in crate::widget) fn update_shared_study_maps(&mut self, rect: Rect) {
        let (start, end) = self.state.visible_range();
        let maps: Vec<_> = self
            .study_ids
            .iter()
            .zip(self.rendered_studies().indicators())
            .map(|(&id, study)| {
                let crate::studies::StudyScale::Shared(key) = study.scale() else {
                    return None;
                };
                let pane = self.series_pane(id)?;
                if pane == PaneId::MAIN {
                    return Some(self.secondary_scale(key).map(
                        self.shared_study_transform(study.as_ref(), key),
                        rect.top(),
                        rect.bottom(),
                    ));
                }
                let primary = if self.config.show_right_axis {
                    SeriesScale::Right
                } else {
                    SeriesScale::Left
                };
                if key == primary {
                    return None;
                }
                let mut low = f64::INFINITY;
                let mut high = f64::NEG_INFINITY;
                for (&other, study) in self
                    .study_ids
                    .iter()
                    .zip(self.rendered_studies().indicators())
                {
                    if self.series_pane(other) == Some(pane)
                        && study.is_visible()
                        && study.scale() == crate::studies::StudyScale::Shared(key)
                    {
                        for value in shared_prices(study.as_ref(), start..end) {
                            low = low.min(value);
                            high = high.max(value);
                        }
                    }
                }
                if !low.is_finite() {
                    return None;
                }
                let pad = crate::scales::auto_scale_span(low, high) * 0.1;
                Some(PriceMap::new(
                    PriceTransform::default(),
                    crate::scales::PriceRange::new(low - pad, high + pad),
                    0.0,
                    1.0,
                    false,
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
        self.secondary_scale(item.options.scale)
            .map(self.series_transform(item), rect.top(), rect.bottom())
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

    pub(in crate::widget) fn series_axis_keys(&self) -> Vec<SeriesScale> {
        let mut keys = Vec::new();
        for item in self.main_pane_series() {
            let key = item.options.scale;
            if key != self.main_series_scale
                && !matches!(key, SeriesScale::Overlay(_))
                && !keys.contains(&key)
            {
                keys.push(key);
            }
        }
        for (_, study) in self.study_ids.iter().zip(self.studies.indicators()) {
            if let crate::studies::StudyScale::Shared(key) = study.scale()
                && study.is_visible()
                && key != self.main_series_scale
                && !matches!(key, SeriesScale::Overlay(_))
                && !keys.contains(&key)
            {
                keys.push(key);
            }
        }
        keys
    }
}

#[cfg(test)]
mod tests;
