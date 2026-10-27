//! Host-fed bar series and their shared price scales.

use super::{Chart, ChartElementId, PaneId, SeriesId};
use crate::chart::renderers::{RenderContext, StyleColors};
use crate::chart::rendering;
use crate::data::FeedState;
use crate::localization as l10n;
use crate::model::{
    AreaStyle, Bar, BaselineStyle, ChartType, ChartTypeParams, ChartTypeStyles, LinePlotStyle,
    LineStyle, Marker, MarkerOptions, MarkerPos, MarkerZOrder, PriceLine, PriceLineSource,
    PriceSource, SeriesScale, SeriesStyle,
};
use crate::scales::StudyFormat;
use crate::scales::{PriceMap, PriceScale, PriceScaleMode, PriceScaleOptions, PriceTransform};
use crate::studies::{Indicator, IndicatorValues, PlotStyle, PlotType};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use chrono::{DateTime, Utc};
use egui::{Color32, Pos2, Rect};

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

/// Display settings of a host-fed bar series.
#[derive(Debug, Clone)]
pub struct SeriesOptions {
    /// Host-supplied title, displayed beside the series values.
    pub name: String,
    /// How the bars are drawn. The types that replace the time axis with their own elements
    /// (range bars, Renko, Kagi, Line Break, Point and Figure) and TPO profiles are drawn as
    /// [`ChartType::Line`] and read back as such.
    pub chart_type: ChartType,
    /// Colour of the line, step, marker and area types, the legend and the last-value tag of
    /// the types drawn from the close.
    pub color: Color32,
    /// Rising bars of the OHLC and column types, and the baseline type above its base.
    pub up_color: Color32,
    /// Falling bars of the OHLC and column types, and the baseline type below its base.
    pub down_color: Color32,
    /// Line width in logical pixels.
    pub width: f32,
    /// Scale shared with the series carrying the same assignment in the price pane. In another
    /// pane the series uses that pane's scale.
    pub scale: SeriesScale,
    /// The price [`ChartType::Columns`] rise above or fall below, which the scale's fit then
    /// includes; `None` raises them from the pane bottom.
    pub columns_base: Option<f64>,
    /// Last value label, price line, base line and autoscale. By default the series labels its
    /// last value without a price line.
    pub style: SeriesStyle,
}

impl Default for SeriesOptions {
    fn default() -> Self {
        Self {
            name: String::new(),
            chart_type: ChartType::Line,
            color: Color32::from_rgb(41, 98, 255),
            up_color: DESIGN_TOKENS.semantic.extended.bullish,
            down_color: DESIGN_TOKENS.semantic.extended.bearish,
            width: 2.0,
            scale: SeriesScale::Right,
            columns_base: None,
            style: SeriesStyle {
                price_line_visible: false,
                ..SeriesStyle::default()
            },
        }
    }
}

impl SeriesOptions {
    fn sanitized(mut self) -> Self {
        if !self.width.is_finite() || self.width <= 0.0 {
            self.width = 2.0;
        }
        if self.chart_type.is_time_independent()
            || self.chart_type == ChartType::TimePriceOpportunity
        {
            self.chart_type = ChartType::Line;
        }
        self
    }

    /// The line, area and baseline styles in the series colours, with the fill opacities of
    /// the default styles, and the columns' base.
    fn type_params(&self) -> ChartTypeParams {
        let line = |color| LinePlotStyle {
            color,
            width: self.width,
            style: LineStyle::Solid,
            ..Default::default()
        };
        let fill = |color: Color32, alpha| {
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
        };
        ChartTypeParams {
            styles: ChartTypeStyles {
                line: line(self.color),
                step_line: line(self.color),
                line_with_markers: line(self.color),
                area: AreaStyle {
                    line: line(self.color),
                    top_fill: fill(self.color, 115),
                    bottom_fill: Color32::TRANSPARENT,
                    ..Default::default()
                },
                baseline: BaselineStyle {
                    above: line(self.up_color),
                    below: line(self.down_color),
                    above_fill: fill(self.up_color, 30),
                    below_fill: fill(self.down_color, 30),
                },
                ..Default::default()
            },
            columns_base: self.columns_base,
            ..Default::default()
        }
    }
}

/// The base line of a percentage or indexed scale mapped by `map`, at 0% or 100.
pub(super) fn base_line(map: &PriceMap) -> Option<PriceLine> {
    let transform = map.transform();
    matches!(
        transform.mode,
        PriceScaleMode::Percentage | PriceScaleMode::IndexedTo100
    )
    .then(|| {
        PriceLine::new(transform.base, DESIGN_TOKENS.semantic.extended.gray)
            .with_width(DESIGN_TOKENS.stroke.hairline)
            .with_style(LineStyle::Solid)
            .with_axis_label(false)
    })
}

pub(super) struct PriceSeries {
    pub id: SeriesId,
    pub options: SeriesOptions,
    pub visible: bool,
    pub bars: Vec<Bar>,
    /// Changes when bars other than the last one change, as the main bars' revision does.
    pub revision: u64,
    pub heikin_ashi: rendering::candles::HeikinAshiCache,
    pub feed: FeedState,
    pub markers: Vec<Marker>,
    pub marker_options: MarkerOptions,
    pub change_marks: Option<super::change_marks::ChangeMarks>,
    pub lines: Vec<(super::PriceLineId, PriceLine)>,
}

impl PriceSeries {
    /// The prices of `bar` that the series draws, lowest first.
    pub(super) fn extremes(&self, bar: &Bar) -> [f64; 2] {
        if self.options.chart_type.uses_ohlc() {
            [bar.low, bar.high]
        } else {
            [bar.close; 2]
        }
    }

    /// The last-value tag colour: the bar's own colour, or its direction for the OHLC types.
    pub(super) fn value_color(&self, bar: &Bar) -> Color32 {
        if let Some(color) = bar.colors.as_ref().and_then(|own| own.color) {
            return color;
        }
        match self.options.chart_type.uses_ohlc() {
            true if bar.close >= bar.open => self.options.up_color,
            true => self.options.down_color,
            false => self.options.color,
        }
    }
}

/// A host series as a member of a pane below or above the price pane, which supplies the
/// pane's legend row and its controls. The chart type renderer draws the bars, so the study
/// holds no values to plot; its output reports the close at the crosshair, or the last close,
/// at every bar.
#[derive(Clone)]
pub(super) struct SeriesLegend {
    name: String,
    style: PlotStyle,
    close: Option<f64>,
    values: IndicatorValues,
    formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    /// Decimals of the series' price scale, when it sets them.
    precision: Option<usize>,
}

impl SeriesLegend {
    pub(super) fn new(
        item: &PriceSeries,
        close: Option<f64>,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
        precision: Option<usize>,
    ) -> Self {
        Self {
            name: item.options.name.clone(),
            style: PlotStyle {
                color: item.options.color,
                width: item.options.width,
                pattern: LineStyle::Solid,
                plot_type: PlotType::Line,
                visible: true,
            },
            close,
            formatter,
            precision,
            values: IndicatorValues::default(),
        }
    }
}

impl Indicator for SeriesLegend {
    fn plot_style(&self, _line: usize) -> PlotStyle {
        self.style
    }

    fn plot_value(&self, _line: usize, _bar: usize) -> Option<f64> {
        self.close
    }

    fn format_value(&self, value: f64) -> String {
        self.formatter.as_ref().map_or_else(
            || StudyFormat::Price.format(value, self.precision),
            |formatter| formatter.format(value),
        )
    }

    fn value_format(&self) -> StudyFormat {
        StudyFormat::Price
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn calculate(&mut self, _data: &[Bar]) {}

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.style.color]
    }

    fn set_colors(&mut self, _colors: Vec<Color32>) {}

    fn line_names(&self) -> Vec<String> {
        vec![self.name.clone()]
    }

    fn set_visible(&mut self, _visible: bool) {}

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}

impl Chart {
    /// Adds a bar series of any time-based chart type, with independent historical and
    /// streaming bars, drawn above the main series in the price pane. Supply data through
    /// [`set_data`](Self::set_data) and [`update`](Self::update); the host owns feeds. Times
    /// are projected onto the main series' time axis, including its whitespace. Move the
    /// series to another pane with [`move_series_to_pane`](Self::move_series_to_pane).
    pub fn add_series(&mut self, options: SeriesOptions) -> SeriesId {
        let options = options.sanitized();
        let id = SeriesId(self.next_series);
        self.next_series += 1;
        self.ensure_series_scale(options.scale);
        self.price_series.push(PriceSeries {
            id,
            options,
            visible: true,
            bars: Vec::new(),
            revision: crate::model::next_revision(),
            heikin_ashi: Default::default(),
            feed: FeedState::default(),
            markers: Vec::new(),
            marker_options: MarkerOptions::default(),
            change_marks: None,
            lines: Vec::new(),
        });
        id
    }

    /// Replaces the display settings of an added bar series, including its chart type, style
    /// and scale assignment, as [`add_series`](Self::add_series) accepts them. Returns `false`
    /// for an unknown id. The main series is styled through
    /// [`set_chart_type`](Self::set_chart_type),
    /// [`set_chart_type_params`](Self::set_chart_type_params), the colours of
    /// [`config_mut`](Self::config_mut) and [`series_style_mut`](Self::series_style_mut), and
    /// assigned with [`set_series_scale`](Self::set_series_scale).
    pub fn set_series_options(&mut self, id: SeriesId, options: SeriesOptions) -> bool {
        let Some(at) = self.price_series.iter().position(|item| item.id == id) else {
            return false;
        };
        let options = options.sanitized();
        self.ensure_series_scale(options.scale);
        self.price_series[at].options = options;
        self.prune_series_scales();
        true
    }

    /// The position of an added bar series in the draw order, which [`series`](Self::series)
    /// follows. Each pane draws its added series in this order: they are drawn above the main
    /// series, and a later one covers an earlier one.
    pub fn series_order(&self, id: SeriesId) -> Option<usize> {
        self.price_series.iter().position(|item| item.id == id)
    }

    /// Moves an added bar series to position `order` of the draw order, or to the end for a
    /// larger value. Returns `false` for an unknown id.
    pub fn set_series_order(&mut self, id: SeriesId, order: usize) -> bool {
        let Some(at) = self.series_order(id) else {
            return false;
        };
        let item = self.price_series.remove(at);
        self.price_series
            .insert(order.min(self.price_series.len()), item);
        true
    }

    /// Adds a comparison line on the main series' scale and switches that scale to percent.
    /// Each series uses its own first visible close as zero; scrolling recomputes references.
    pub fn add_comparison(&mut self, name: impl Into<String>, color: Color32) -> SeriesId {
        self.state.price_scale_mut().options_mut().mode = PriceScaleMode::Percentage;
        self.state.price_scale_mut().reset();
        self.add_series(SeriesOptions {
            name: name.into(),
            color,
            scale: self.main_series_scale,
            ..Default::default()
        })
    }

    /// Added bar series and their display settings, in draw order.
    pub fn series(&self) -> impl Iterator<Item = (SeriesId, &SeriesOptions)> {
        self.price_series
            .iter()
            .map(|item| (item.id, &item.options))
    }

    /// Whether a bar series or study is present and visible.
    pub fn is_series_visible(&self, id: SeriesId) -> bool {
        if id == SeriesId::MAIN {
            self.main_series_present && self.main_series_visible
        } else if let Some(item) = self.price_series.iter().find(|item| item.id == id) {
            item.visible
        } else {
            self.indicators()
                .any(|(current, study)| current == id && study.is_visible())
        }
    }

    /// Shows or hides a bar series or study without unloading its data.
    /// Returns `false` when the id is absent. Hidden series do not contribute to auto-scaling.
    pub fn set_series_visible(&mut self, id: SeriesId, visible: bool) -> bool {
        if id == SeriesId::MAIN {
            if !self.main_series_present {
                return false;
            }
            self.set_main_series_visible(visible);
        } else if let Some(item) = self.price_series.iter_mut().find(|item| item.id == id) {
            item.visible = visible;
            if !visible && self.selection.selected_id() == Some(ChartElementId::Series(id)) {
                self.selection.deselect();
            }
        } else {
            return self.set_indicator_visible(id, visible);
        }
        true
    }

    /// Assigns a bar series to the right, left, a hidden overlay or an additional labelled scale.
    /// Shared assignments share options and manual ranges. Other series retain their assignments.
    pub fn set_series_scale(&mut self, id: SeriesId, scale: SeriesScale) -> bool {
        if id == SeriesId::MAIN && self.main_series_present {
            if scale == self.main_series_scale {
                return true;
            }
            self.ensure_series_scale(scale);
            let next = self
                .series_scales
                .remove(&scale)
                .expect("scale was inserted");
            let old = std::mem::replace(self.state.price_scale_mut(), next);
            // Joining a scale preserves its bounds in scale units, even with a new reference.
            if self.state.price_scale().is_manual() {
                let range = self.state.price_scale().range();
                self.state.fit_price_scale(std::iter::empty());
                self.state.price_scale_mut().set_range(range.min, range.max);
            }
            self.series_scales.insert(self.main_series_scale, old);
            self.main_series_scale = scale;
            self.config.show_left_axis = scale == SeriesScale::Left;
            self.config.show_right_axis =
                matches!(scale, SeriesScale::Right | SeriesScale::Separate(_));
        } else if let Some(at) = self.price_series.iter().position(|item| item.id == id) {
            self.ensure_series_scale(scale);
            self.price_series[at].options.scale = scale;
        } else {
            return false;
        }
        self.prune_series_scales();
        true
    }

    /// The scale assignment of a present bar series.
    pub fn series_scale(&self, id: SeriesId) -> Option<SeriesScale> {
        if id == SeriesId::MAIN && self.main_series_present {
            Some(self.main_series_scale)
        } else {
            self.price_series
                .iter()
                .find(|item| item.id == id)
                .map(|item| item.options.scale)
        }
    }

    /// The last value label, price line, base line, autoscale and click tolerance of a present
    /// bar series.
    pub fn series_style(&self, id: SeriesId) -> Option<&SeriesStyle> {
        if id == SeriesId::MAIN && self.main_series_present {
            Some(&self.main_series_style)
        } else {
            self.price_series
                .iter()
                .find(|item| item.id == id)
                .map(|item| &item.options.style)
        }
    }

    /// Changes the last value label, price line, base line, autoscale or click tolerance of a
    /// present bar series. Changes take effect on the next rendered frame.
    pub fn series_style_mut(&mut self, id: SeriesId) -> Option<&mut SeriesStyle> {
        if id == SeriesId::MAIN && self.main_series_present {
            Some(&mut self.main_series_style)
        } else {
            self.price_series
                .iter_mut()
                .find(|item| item.id == id)
                .map(|item| &mut item.options.style)
        }
    }

    /// The scale shared by this present bar series, including its options and fitted range.
    pub fn series_price_scale(&self, id: SeriesId) -> Option<&PriceScale> {
        let scale = self.series_scale(id)?;
        Some(
            match self
                .pane_layout
                .panes
                .iter()
                .find(|pane| pane.members.contains(&id))
            {
                Some(pane) => &pane.scale,
                None => self.secondary_scale(scale),
            },
        )
    }

    /// Changes options or a manual range on the scale of this bar series, as returned by
    /// [`series_price_scale`](Self::series_price_scale).
    /// Range bounds use scale units, such as percent in percentage mode.
    pub fn series_price_scale_mut(&mut self, id: SeriesId) -> Option<&mut PriceScale> {
        let scale = self.series_scale(id)?;
        if let Some(at) = self
            .pane_layout
            .panes
            .iter()
            .position(|pane| pane.members.contains(&id))
        {
            Some(&mut self.pane_layout.panes[at].scale)
        } else if scale == self.main_series_scale {
            Some(self.state.price_scale_mut())
        } else {
            self.series_scales.get_mut(&scale)
        }
    }

    /// The price mapping of a present bar series, the main one included, as last drawn in its
    /// pane: [`PriceMap::price_to_y`] and [`PriceMap::y_to_price`] convert between its prices
    /// and the y coordinates of the chart's [`Ui`](egui::Ui). `None` for an absent series, or
    /// before its pane is first drawn.
    pub fn series_price_map(&self, id: SeriesId) -> Option<PriceMap> {
        if self.series_pane(id)? != PaneId::MAIN {
            return self
                .pane_layout
                .panes
                .iter()
                .find(|pane| pane.members.contains(&id) && pane.rect.is_positive())
                .map(|pane| pane.price_map());
        }
        let rect = self.last_rendered_price_rect;
        if !rect.is_positive() {
            None
        } else if id == SeriesId::MAIN {
            Some(self.last_price_map)
        } else {
            let item = self.price_series.iter().find(|item| item.id == id)?;
            Some(self.series_map(item, rect))
        }
    }

    /// Formats `price` as the scale of a present bar series labels it: a price, a signed
    /// percentage or an indexed value. `None` where [`series_price_map`](Self::series_price_map)
    /// is.
    pub fn format_series_price(&self, id: SeriesId, price: f64) -> Option<String> {
        let map = self.series_price_map(id)?;
        let transform = map.transform();
        Some(transform.label(
            transform.to_scale(price),
            map.price_precision().unwrap_or(2),
            self.series_label_formatter(id, transform.mode).as_deref(),
        ))
    }

    pub(super) fn ensure_series_scale(&mut self, scale: SeriesScale) {
        if scale != self.main_series_scale {
            self.series_scales
                .entry(scale)
                .or_insert_with(|| PriceScale::new(PriceScaleOptions::default()));
        }
    }

    pub(super) fn prune_series_scales(&mut self) {
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

    fn secondary_scale(&self, scale: SeriesScale) -> &PriceScale {
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

    pub(super) fn series_range(&self, item: &PriceSeries) -> std::ops::Range<usize> {
        match self.visible_times() {
            Some((first, last)) => {
                item.bars.partition_point(|bar| bar.time < first)
                    ..item.bars.partition_point(|bar| bar.time <= last)
            }
            None => 0..0,
        }
    }

    /// The bar whose close the series' price line and last value label show.
    fn series_last_bar<'a>(&self, item: &'a PriceSeries) -> Option<&'a Bar> {
        match item.options.style.price_line_source {
            PriceLineSource::LastBar => crate::model::last_with_value(&item.bars),
            PriceLineSource::LastVisible => {
                crate::model::last_with_value(&item.bars[self.series_range(item)])
            }
        }
    }

    /// The prices of the price-anchored `markers` in view, when `options` fit them.
    pub(super) fn marker_prices<'a>(
        &self,
        markers: impl IntoIterator<Item = &'a Marker>,
        options: MarkerOptions,
    ) -> impl Iterator<Item = f64> {
        let window = self.visible_times().filter(|_| options.auto_scale);
        markers.into_iter().filter_map(move |marker| {
            let (first, last) = window?;
            let anchored = matches!(
                marker.position,
                MarkerPos::AtPriceTop | MarkerPos::AtPriceBottom | MarkerPos::AtPriceMiddle
            );
            (anchored && (first..=last).contains(&marker.time))
                .then_some(marker.price)
                .flatten()
        })
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
    pub(super) fn main_pane_series(&self) -> impl DoubleEndedIterator<Item = &PriceSeries> {
        self.price_series
            .iter()
            .filter(|item| item.visible && !self.in_pane(item.id))
    }

    /// The close of `item` at the time of the shown bar `bar`, if it has a bar then.
    pub(super) fn series_close_at(&self, item: &PriceSeries, bar: usize) -> Option<f64> {
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
    pub(super) fn series_fit(&self, item: &PriceSeries) -> Option<(f64, f64)> {
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
    pub(super) fn fit_secondary_series(&mut self) -> Vec<f64> {
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
            let markers = self.marker_prices(&item.markers, item.marker_options);
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

    pub(super) fn update_shared_study_maps(&mut self, rect: Rect) {
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

    pub(super) fn series_map(&self, item: &PriceSeries, rect: Rect) -> PriceMap {
        self.secondary_scale(item.options.scale).map(
            self.series_transform(item),
            rect.top(),
            rect.bottom(),
        )
    }

    pub(super) fn secondary_axis_values(
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
    pub(super) fn series_tag_label(
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
            transform.label(
                transform.to_scale(value),
                map.price_precision().unwrap_or(2),
                self.series_label_formatter(item.id, transform.mode)
                    .as_deref(),
            ),
        )
    }

    /// The price-line labels and the last close that `item` shows on its price axis, each with
    /// its tag colours and whether it is the last close.
    pub(super) fn series_axis_values<'a>(
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

    pub(super) fn series_axis_keys(&self) -> Vec<SeriesScale> {
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

    pub(super) fn hit_price_series(
        &self,
        item: &PriceSeries,
        pos: Pos2,
        rect: Rect,
        map: PriceMap,
    ) -> Option<usize> {
        if !rect.contains(pos) {
            return None;
        }
        let main = self.state.shown();
        let logical =
            self.state
                .time_scale()
                .coord_to_idx_precise(pos.x, rect.left(), rect.width());
        let time = crate::model::time_at(main, logical)?;
        let at = item.bars.partition_point(|bar| bar.time < time);
        let x = |bar: &Bar| {
            crate::model::index_at(main, bar.time).map(|index| {
                self.state
                    .time_scale()
                    .idx_to_coord_precise(index, rect.left(), rect.width())
            })
        };
        let tolerance = item.options.style.hit_tolerance;
        for index in at.saturating_sub(1)..(at + 2).min(item.bars.len()) {
            let bar = &item.bars[index];
            if bar.is_whitespace() {
                continue;
            }
            let px = x(bar)?;
            let close = Pos2::new(px, map.price_to_y(bar.close));
            let hit = if item.options.chart_type.uses_ohlc() {
                let half_width = self.state.time_scale().bar_spacing() * 0.35;
                let body = Rect::from_two_pos(
                    Pos2::new(px - half_width, map.price_to_y(bar.open)),
                    Pos2::new(px + half_width, close.y),
                );
                body.expand(1.0).contains(pos)
                    || ((pos.x - px).abs() <= tolerance
                        && (map.price_to_y(bar.low).min(map.price_to_y(bar.high))
                            ..=map.price_to_y(bar.low).max(map.price_to_y(bar.high)))
                            .contains(&pos.y))
            } else if item.options.chart_type == ChartType::Columns {
                let base = item
                    .options
                    .columns_base
                    .map_or(rect.bottom(), |base| map.price_to_y(base));
                Rect::from_two_pos(
                    Pos2::new(px - self.state.time_scale().bar_spacing() * 0.4, base),
                    Pos2::new(px + self.state.time_scale().bar_spacing() * 0.4, close.y),
                )
                .contains(pos)
            } else if pos.distance(close) <= tolerance {
                true
            } else if let Some(next) = item.bars.get(index + 1).filter(|bar| !bar.is_whitespace()) {
                let next = Pos2::new(x(next)?, map.price_to_y(next.close));
                if item.options.chart_type == ChartType::StepLine {
                    let corner = Pos2::new(next.x, close.y);
                    crate::chart::hit_test::point_to_segment_distance(pos, close, corner).min(
                        crate::chart::hit_test::point_to_segment_distance(pos, corner, next),
                    ) <= tolerance
                } else {
                    crate::chart::hit_test::point_to_segment_distance(pos, close, next) <= tolerance
                }
            } else {
                false
            };
            if hit {
                return crate::model::index_at(main, bar.time)
                    .map(|index| index.round().max(0.0) as usize);
            }
        }
        None
    }

    /// Paints the bars, price lines, change marks and markers of `item` in `rect` with `map`.
    /// Markers placed above the series are left to `paint_marks` on the price pane; in another
    /// pane they are painted here, after the pane's studies.
    pub(super) fn paint_series(
        &self,
        plot: &egui::Painter,
        rect: Rect,
        item: &PriceSeries,
        map: PriceMap,
    ) {
        let main = self.state.shown();
        if main.is_empty() {
            return;
        }
        let range = self.series_range(item);
        // One neighbour at each edge keeps clipped segments continuous during scrolling.
        let start = range.start.saturating_sub(1);
        let end = (range.end + 1).min(item.bars.len());
        let visible = &item.bars[start..end];
        let options = &item.options;
        let colors = StyleColors {
            bullish: options.up_color,
            bearish: options.down_color,
            text: self.config.text_color,
            bullish_border: None,
            bearish_border: None,
            bullish_wick: None,
            bearish_wick: None,
            candle_border_width: 0.0,
        };
        let context = RenderContext::new(plot, rect);
        let clock = self.bar_clock();
        let type_params = options.type_params();
        // The renderers place the series' bars by index; each index maps through its bar's
        // time onto the main time axis.
        let x = |index: usize, _| {
            item.bars
                .get(index)
                .and_then(|bar| crate::model::index_at(main, bar.time))
                .map_or(f32::NAN, |at| {
                    self.state
                        .time_scale()
                        .idx_to_coord_precise(at, rect.left(), rect.width())
                })
        };
        rendering::render_chart_type(
            options.chart_type,
            &rendering::CandleDataContext {
                price_ctx: &context,
                volume_ctx: &context,
                price_scale: &map,
                colors: &colors,
                visible_data: visible,
                full_data: &item.bars,
                revision: item.revision,
                heikin_ashi: &item.heikin_ashi,
                start_idx: start,
                clock: &clock,
                tick_size: None,
            },
            &rendering::ChartTypeParams::new(
                rendering::BarDimensions::new(
                    self.state.time_scale().bar_spacing() * self.config.candle_width,
                    self.config.wick_width,
                ),
                rendering::VolumeSettings::new(
                    false,
                    visible
                        .iter()
                        .map(|bar| bar.volume)
                        .filter(|volume| volume.is_finite())
                        .max_by(f64::total_cmp)
                        .unwrap_or(1.0),
                ),
                &type_params,
                rendering::TradingColors::new(options.up_color, options.down_color),
                rendering::CoordMapping::new(rect.left()),
                PriceSource::Close,
            ),
            x,
        );
        let style = &item.options.style;
        let price_line = self
            .series_last_bar(item)
            .filter(|_| self.config.show_price_line && style.price_line_visible)
            .map(|bar| {
                PriceLine::new(
                    bar.close,
                    style.price_line_color.unwrap_or(item.value_color(bar)),
                )
                .with_width(style.price_line_width)
                .with_style(style.price_line_style)
                .with_axis_label(false)
            });
        let base_line = base_line(&map).filter(|_| style.base_line_visible);
        let lines = item.lines.iter().map(|(_, line)| line);
        for line in base_line.iter().chain(&price_line).chain(lines) {
            crate::chart::rendering::render_price_line(
                plot,
                rect,
                &map,
                line,
                rect.left(),
                false,
                rect.right(),
            );
        }
        if let Some(marks) = &item.change_marks {
            marks.paint(plot, |time, close| {
                let index = crate::model::index_at(main, time)?;
                let x =
                    self.state
                        .time_scale()
                        .idx_to_coord_precise(index, rect.left(), rect.width());
                Some(Pos2::new(x, map.price_to_y(close)))
            });
        }
        if item.marker_options.z_order == MarkerZOrder::Normal || self.in_pane(item.id) {
            self.paint_series_markers(plot, rect, item, map);
        }
    }

    /// Paints the markers of `item` with `map`, each at the time and price of its bar.
    pub(super) fn paint_series_markers(
        &self,
        plot: &egui::Painter,
        rect: Rect,
        item: &PriceSeries,
        map: PriceMap,
    ) {
        for marker in &item.markers {
            let Some(index) = crate::model::index_at(self.state.shown(), marker.time) else {
                continue;
            };
            let at = item
                .bars
                .partition_point(|bar| bar.time <= marker.time)
                .saturating_sub(1);
            if let Some(bar) = item.bars.get(at) {
                let coords = crate::chart::coords::ChartMapping {
                    start_idx: 0,
                    right_offset: self.get_chart_mapping().right_offset - index,
                    price: map,
                    ..self.get_chart_mapping()
                };
                let bar = Bar {
                    time: marker.time,
                    ..bar.clone()
                };
                crate::chart::renderers::render_markers(
                    &RenderContext::new(plot, rect),
                    std::slice::from_ref(marker),
                    &[bar],
                    &map,
                    &coords,
                );
            }
        }
    }

    /// Paints a legend row for each of `items`, the first at `top`. The values follow a pointer
    /// over `rect`. Returns the bottom of the last row, or `top` without rows.
    pub(super) fn paint_secondary_legend<'a>(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        top: f32,
        items: impl Iterator<Item = &'a PriceSeries>,
    ) -> f32 {
        let plot = painter.with_clip_rect(painter.clip_rect().intersect(rect));
        let font = egui::FontId::proportional(ui.active_theme().text.ohlc);
        let row_height = painter
            .layout_no_wrap("Ag".into(), font.clone(), self.config.text_color)
            .size()
            .y
            + ui.active_theme().spacing.sm;
        let main = self.state.shown();
        let mut bottom = top;
        for (row, item) in items.enumerate() {
            if self.config.show_indicator_labels || self.config.show_indicator_last_val {
                let selected_time = ui
                    .input(|input| input.pointer.hover_pos())
                    .filter(|point| rect.contains(*point))
                    .and_then(|point| {
                        main.get(
                            self.state
                                .time_scale()
                                .coord_to_idx_precise(point.x, rect.left(), rect.width())
                                .round()
                                .max(0.0) as usize,
                        )
                    })
                    .map(|bar| bar.time);
                let bar = match selected_time {
                    Some(time) => item
                        .bars
                        .binary_search_by_key(&time, |bar| bar.time)
                        .ok()
                        .map(|at| &item.bars[at])
                        .filter(|bar| !bar.is_whitespace()),
                    None => crate::model::last_with_value(&item.bars),
                };
                let formatter = self.series_label_formatter(item.id, PriceScaleMode::Normal);
                let value = bar.map_or_else(
                    || "∅".into(),
                    |bar| {
                        formatter.as_ref().map_or_else(
                            || {
                                format!(
                                    "{:.*}",
                                    self.series_map(item, rect).price_precision().unwrap_or(2),
                                    bar.close
                                )
                            },
                            |formatter| formatter.format(bar.close),
                        )
                    },
                );
                let label = plot.text(
                    Pos2::new(rect.left() + 8.0, top + row as f32 * row_height),
                    egui::Align2::LEFT_TOP,
                    format!(
                        "{}  {}",
                        if self.config.show_indicator_labels {
                            &item.options.name
                        } else {
                            ""
                        },
                        if self.config.show_indicator_last_val {
                            &value
                        } else {
                            ""
                        }
                    ),
                    font.clone(),
                    item.options.color,
                );
                bottom = label.bottom();
            }
        }
        bottom
    }

    pub(super) fn secondary_axes(&self, widget: Rect, rect: Rect) -> Vec<(SeriesScale, Rect)> {
        let keys = self.series_axis_keys();
        let left_count = keys
            .iter()
            .filter(|key| **key != SeriesScale::Right)
            .count();
        let primary_left = if self.config.show_left_axis {
            self.config.left_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        };
        let primary_right = if self.config.show_right_axis {
            self.config.right_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        };
        let study_count = self
            .studies
            .indicators()
            .iter()
            .filter(|study| {
                (self.config.show_left_axis || self.config.show_right_axis)
                    && study.is_visible()
                    && study.is_overlay()
                    && !study.is_volume()
                    && study.scale() == crate::studies::StudyScale::Independent
            })
            .count();
        let padding = self
            .config
            .padding
            .max(0.0)
            .min(widget.width().min(widget.height()).max(0.0) / 2.0);
        let mut left = (rect.left() - primary_left).max(widget.left() + padding);
        let slot_width =
            (left - widget.left() - padding).max(0.0) / (left_count + study_count).max(1) as f32;
        keys.into_iter()
            .map(|key| {
                let axis = if key == SeriesScale::Right {
                    Rect::from_min_max(
                        Pos2::new(
                            (rect.right() + primary_right).min(widget.right()),
                            rect.top(),
                        ),
                        Pos2::new(widget.right(), rect.bottom()),
                    )
                } else {
                    let axis = Rect::from_min_max(
                        Pos2::new(left - slot_width, rect.top()),
                        Pos2::new(left, rect.bottom()),
                    );
                    left -= slot_width;
                    axis
                };
                (key, axis)
            })
            .collect()
    }

    pub(super) fn handle_secondary_axes(&mut self, ui: &egui::Ui, widget: Rect, rect: Rect) {
        for (key, axis) in self.secondary_axes(widget, rect) {
            let response = ui.interact(
                axis,
                ui.scope_id().with(("series_axis", key)),
                egui::Sense::drag(),
            );
            let Some(scale) = self.series_scales.get_mut(&key) else {
                continue;
            };
            if response.double_clicked()
                && self
                    .chart_options
                    .handle_scale
                    .axis_double_click_reset
                    .price
            {
                scale.reset();
            } else if response.dragged()
                && self
                    .chart_options
                    .handle_scale
                    .axis_pressed_mouse_move
                    .price
            {
                let delta = ui.input(|input| input.pointer.delta().y);
                let range = scale.range();
                let anchor = (range.min + range.max) * 0.5;
                scale.zoom(
                    (f64::from(delta) / f64::from(rect.height().max(1.0))).exp(),
                    anchor,
                );
            }
        }
    }

    pub(super) fn paint_series_axes(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        widget: Rect,
        rect: Rect,
        colors: &StyleColors,
    ) -> f32 {
        let study_axis = crate::config::PriceAxisOptions {
            show_study_values: self.config.show_indicator_last_val
                && self.config.price_axis.show_study_values,
            ..self.config.price_axis
        };
        let mut measured = 0.0_f32;
        for (key, axis) in self.secondary_axes(widget, rect) {
            let position = if key == SeriesScale::Right {
                crate::scales::PriceScaleId::Right
            } else {
                crate::scales::PriceScaleId::Left
            };
            let clipped = painter.with_clip_rect(painter.clip_rect().intersect(axis));
            let assigned: Vec<_> = self
                .rendered_studies()
                .indicators()
                .iter()
                .filter(|study| {
                    study.is_visible()
                        && study.is_overlay()
                        && study.scale() == crate::studies::StudyScale::Shared(key)
                })
                .map(|study| study.as_ref())
                .collect();
            let Some(item) = self
                .main_pane_series()
                .find(|item| item.options.scale == key)
            else {
                if let Some(study) = assigned.first() {
                    let coords = crate::chart::renderers::ChartMapping {
                        rect,
                        price: study.scale_mapping().unwrap_or(self.last_price_map),
                        ..self.get_chart_mapping()
                    };
                    measured = measured.max(crate::chart::renderers::render_study_axis(
                        painter,
                        &assigned,
                        &coords,
                        axis,
                        &study_axis,
                        self.percentage_formatter.as_deref(),
                    ));
                }
                continue;
            };
            let map = self.series_map(item, rect);
            let label_rect = if key == SeriesScale::Right {
                Rect::from_min_max(rect.min, axis.left_bottom())
            } else {
                Rect::from_min_max(axis.right_top(), rect.max)
            };
            let ctx = RenderContext::new(&clipped, label_rect);
            let slot = clipped.add(egui::Shape::Noop);
            let mut tags = Vec::new();
            for (value, colors, mapping, label) in self.secondary_axis_values(key, rect) {
                tags.push(crate::chart::rendering::axes::render_axis_value(
                    &clipped,
                    label_rect,
                    axis.right(),
                    mapping.price_to_y(value),
                    label,
                    colors,
                    &tags,
                    self.config.price_axis.allow_label_overlap,
                ));
            }
            if study_axis.show_study_values {
                for study in &assigned {
                    let mapping = study
                        .scale_mapping()
                        .unwrap_or(map)
                        .with_bounds(rect.top(), rect.bottom());
                    crate::chart::rendering::axes::render_study_tags(
                        &clipped,
                        label_rect,
                        &mapping,
                        *study,
                        axis.right(),
                        &mut tags,
                        None,
                        &study_axis,
                    );
                }
            }
            let formatter = self.series_label_formatter(item.id, map.transform().mode);
            crate::chart::renderers::render_price_labels_at_pos(
                &ctx,
                &map,
                colors,
                &self.config.price_axis,
                position,
                &tags,
                slot,
                formatter.as_deref(),
            );
            measured = measured.max(crate::chart::rendering::axes::price_axis_width(
                &clipped,
                &map,
                &[],
                formatter.as_deref(),
            ));
            ui.interact(
                axis,
                ui.scope_id().with(("series_axis", key)),
                egui::Sense::drag(),
            )
            .on_hover_text(l10n::text(ui.ctx(), &item.options.name));
        }
        measured
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BarData;
    use crate::widget::test_frames::frame;
    use chrono::{TimeDelta, Utc};

    fn bars(prices: &[f64]) -> Vec<Bar> {
        let start = "2024-01-01T00:00:00Z"
            .parse::<chrono::DateTime<Utc>>()
            .unwrap();
        prices
            .iter()
            .enumerate()
            .map(|(index, &price)| {
                Bar::new(
                    start + TimeDelta::minutes(index as i64),
                    price,
                    price,
                    price,
                    price,
                    1.0,
                )
            })
            .collect()
    }

    /// Logarithms are monotone on positive prices. With 10% margins, ln(2)..ln(100)
    /// occupies 80% of the pane; a lone zero-crossing bar instead starts at ln(5).
    #[test]
    fn log_fit_includes_positive_ohlc_of_zero_crossing_bars() {
        for secondary in [false, true] {
            for count in [1, 2] {
                let mut input = bars(&[10.0, 15.0]);
                input[0].open = 5.0;
                input[0].high = 100.0;
                input[0].low = -1.0;
                input[1].high = 20.0;
                input[1].low = 2.0;
                input.truncate(count);
                let mut chart = Chart::new(BarData::from_bars(input.clone()));
                let id = if secondary {
                    let id = chart.add_series(SeriesOptions {
                        chart_type: ChartType::Candles,
                        scale: SeriesScale::Left,
                        ..Default::default()
                    });
                    chart.set_data(id, input);
                    id
                } else {
                    SeriesId::MAIN
                };
                chart.series_price_scale_mut(id).unwrap().options_mut().mode =
                    PriceScaleMode::Logarithmic;
                frame(&egui::Context::default(), &mut chart, 0.0, vec![]);
                let map = chart.series_price_map(id).unwrap();
                let low: f64 = if count == 1 { 5.0 } else { 2.0 };
                let span = (100.0_f64.ln() - low.ln()) / 0.8;
                assert!(
                    (map.range().min - (low.ln() - span * 0.1)).abs() < 1e-10,
                    "secondary={secondary}, count={count}: {:?}",
                    map.range()
                );
                assert!((map.range().max - (100.0_f64.ln() + span * 0.1)).abs() < 1e-10);
                for price in [low, 5.0, 10.0, 100.0] {
                    let y = map.price_to_y(price);
                    let (top, bottom) = map.extent();
                    assert!((top..=bottom).contains(&y), "{price}: {y}");
                }
            }
        }
    }

    /// The first transformed open after the gap is (100 + 100) / 2 = 100,
    /// although its raw prices are between 9 and 11. A right edge at 1.5 also fits bar 2.
    #[test]
    fn heikin_ashi_fit_uses_the_transformed_visible_candles() {
        for (secondary, right, high) in [
            (false, 8.0, 100.0),
            (true, 8.0, 100.0),
            (false, 1.5, 1001.0),
            (true, 1.5, 1001.0),
        ] {
            let mut input = bars(&[10.0; 9]);
            for bar in &mut input {
                bar.high = 11.0;
                bar.low = 9.0;
            }
            input[0].open = 100.0;
            input[0].close = 100.0;
            input[0].high = 101.0;
            input[0].low = 99.0;
            if right == 1.5 {
                input[2].open = 1000.0;
                input[2].close = 1000.0;
                input[2].high = high;
                input[2].low = 999.0;
            }
            let mut chart = Chart::new(BarData::from_bars(input.clone()));
            let id = if secondary {
                let id = chart.add_series(SeriesOptions {
                    chart_type: ChartType::Heikin,
                    scale: SeriesScale::Left,
                    ..Default::default()
                });
                chart.set_data(id, input);
                id
            } else {
                chart.set_chart_type(ChartType::Heikin);
                SeriesId::MAIN
            };
            let ctx = egui::Context::default();
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.set_visible_logical_range(crate::model::LogicalRange::new(1.0, right));
            let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
            let map = chart.series_price_map(id).unwrap();
            assert!(
                (map.range().min - (9.0 - (high - 9.0) / 8.0)).abs() < 1e-8,
                "secondary={secondary}, right={right}: {:?}",
                map.range()
            );
            assert!((map.range().max - (high + (high - 9.0) / 8.0)).abs() < 1e-8);
            for price in [9.0, 10.0, 100.0, high] {
                let (top, bottom) = map.extent();
                assert!((top..=bottom).contains(&map.price_to_y(price)));
            }
            let x = chart.index_to_x(1.0).unwrap();
            assert!(
                painted.iter().any(|shape| matches!(shape,
                    egui::Shape::Rect(body)
                        if (body.rect.center().x - x).abs() < 1e-3
                            && (body.rect.top() - map.price_to_y(100.0)).abs() < 1e-3
                            && (body.rect.bottom() - map.price_to_y(10.0)).abs() < 1e-3
                )),
                "secondary={secondary}: the first visible body must span 10..100"
            );
        }
    }

    /// Percent compares returns, not absolute prices: 100 -> 110 and 200 -> 220
    /// coincide at +10%; 200 -> 240 extends the shared range to 20%, plus an eighth of that on
    /// each side for the 10% of the pane above and below.
    #[test]
    fn comparisons_share_percent_ranges_with_individual_visible_references() {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0, 120.0])));
        let id = chart.add_comparison("Other", Color32::BLUE);
        chart.set_data(id, bars(&[200.0, 220.0, 240.0]));
        frame(&ctx, &mut chart, 0.0, vec![]);
        let rect = chart.last_rendered_price_rect;
        let map = chart.series_map(&chart.price_series[0], rect);
        assert!((map.price_to_y(220.0) - chart.last_price_map.price_to_y(110.0)).abs() < 1e-4);
        let range = chart.state.price_scale().range();
        assert!(
            (range.min + 2.5).abs() < 1e-10 && (range.max - 22.5).abs() < 1e-10,
            "{range:?}"
        );
        chart.state.time_scale_mut().set_bar_spacing(100.0);
        chart.state.time_scale_mut().set_right_offset(0.0);
        for width in [200.0, 150.0] {
            chart.state.time_scale_mut().set_width(width);
            // Logical left edges 1 and 1.5 both retain the partially visible bar at 1.
            assert_eq!(chart.series_transform(&chart.price_series[0]).base, 220.0);
        }
        chart.state.time_scale_mut().set_bar_spacing(6.0);
        // A missing first timestamp uses the series' first available visible close.
        chart.set_data(id, bars(&[200.0, 220.0, 240.0])[1..].to_vec());
        frame(&ctx, &mut chart, 0.1, vec![]);
        assert_eq!(chart.series_transform(&chart.price_series[0]).base, 220.0);
        chart.set_data(id, bars(&[f64::NAN]));
        frame(&ctx, &mut chart, 0.2, vec![]);
        let map = chart.series_map(&chart.price_series[0], rect);
        assert!(!map.price_to_y(f64::NAN).is_finite());
        for mode in [PriceScaleMode::Percentage, PriceScaleMode::IndexedTo100] {
            chart.price_scale_mut().options_mut().mode = mode;
            chart.set_data(id, bars(&[0.0, 1.0, 1.0]));
            frame(&ctx, &mut chart, 0.25, vec![]);
            let map = chart.series_price_map(id).unwrap();
            assert_eq!(map.transform(), PriceTransform::default());
            for price in [0.0, 1.0] {
                let y = map.price_to_y(price);
                assert!(y.is_finite(), "{mode:?}, {price}: {y}");
                assert!((rect.top()..=rect.bottom()).contains(&y));
            }
        }
        for change in ["hide", "remove", "empty", "join_main"] {
            let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
            let first = chart.add_comparison("First", Color32::BLUE);
            let second = chart.add_comparison("Second", Color32::GREEN);
            for (id, prices) in [(first, [200.0, 220.0]), (second, [400.0, 440.0])] {
                chart.set_series_scale(id, SeriesScale::Left);
                chart.set_data(id, bars(&prices));
            }
            chart
                .series_price_scale_mut(first)
                .unwrap()
                .options_mut()
                .mode = PriceScaleMode::Percentage;
            frame(&ctx, &mut chart, 0.2, vec![]);
            chart
                .series_price_scale_mut(first)
                .unwrap()
                .set_range(-5.0, 25.0);
            match change {
                "hide" => {
                    chart.set_series_visible(first, false);
                }
                "remove" => {
                    chart.remove_series(first);
                }
                "join_main" => {
                    chart.set_series_scale(SeriesId::MAIN, SeriesScale::Left);
                }
                _ => chart.set_data(first, Vec::new()),
            }
            frame(&ctx, &mut chart, 0.3, vec![]);
            let range = chart.series_price_scale(second).unwrap().range();
            assert_eq!((range.min, range.max), (-5.0, 25.0), "{change}");
            if change == "join_main" {
                chart.set_series_scale(SeriesId::MAIN, SeriesScale::Right);
                frame(&ctx, &mut chart, 0.4, vec![]);
                let range = chart.series_price_scale(second).unwrap().range();
                assert_eq!((range.min, range.max), (-5.0, 25.0), "main leaves scale");
            }
            chart
                .series_price_scale_mut(second)
                .unwrap()
                .options_mut()
                .mode = PriceScaleMode::Normal;
            frame(&ctx, &mut chart, 0.5, vec![]);
            let range = chart.series_price_scale(second).unwrap().range();
            let expected = if change == "join_main" {
                (190.0, 250.0)
            } else {
                (380.0, 500.0)
            };
            assert_eq!((range.min, range.max), expected, "mode change: {change}");
        }
    }

    /// Secondary history and corrections use the main merge rules, and visibility/removal
    /// cannot lose data or resurrect an id. A scale move preserves the other memberships.
    #[test]
    fn bar_series_keep_independent_data_and_scale_membership() {
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
        let first = chart.add_series(SeriesOptions::default());
        let second = chart.add_series(SeriesOptions {
            scale: SeriesScale::Left,
            ..Default::default()
        });
        let data = bars(&[200.0, 220.0]);
        chart.set_data(
            first,
            vec![
                data[1].clone(),
                data[0].clone(),
                bars(&[f64::NAN]).remove(0),
            ],
        );
        chart.update(first, bars(&[210.0]).remove(0));
        chart.update(second, data[1].clone());
        assert_eq!(
            chart
                .bars(first)
                .iter()
                .map(|bar| bar.close)
                .collect::<Vec<_>>(),
            [210.0, 220.0]
        );
        assert_eq!(chart.feed_state(first).unwrap().rejected_bars, 1);
        assert_eq!(chart.bars(second), &data[1..]);
        assert_eq!(chart.bars(SeriesId::MAIN), bars(&[100.0, 110.0]));
        for scale in [
            SeriesScale::Left,
            SeriesScale::Overlay(4),
            SeriesScale::Separate(7),
            SeriesScale::Right,
        ] {
            assert!(chart.set_series_scale(first, scale));
            assert_eq!(chart.series_scale(first), Some(scale));
            assert_eq!(chart.series_scale(second), Some(SeriesScale::Left));
            assert!(chart.set_series_visible(first, false));
            assert_eq!(chart.bars(first).len(), 2);
            assert!(!chart.is_series_visible(first));
            assert!(chart.set_series_visible(first, true));
        }
        assert!(chart.set_series_scale(SeriesId::MAIN, SeriesScale::Left));
        assert_eq!(chart.series_scale(first), Some(SeriesScale::Right));
        assert!(chart.remove_series(first));
        assert!(!chart.remove_series(first));
        chart.update(first, data[0].clone());
        assert!(chart.bars(first).is_empty());
        assert!(chart.remove_series(SeriesId::MAIN));
        assert!(!chart.remove_series(SeriesId::MAIN));
        assert!(!chart.set_series_visible(SeriesId::MAIN, true));
        assert!(chart.bars(SeriesId::MAIN).is_empty());
        assert_eq!(chart.bars(second), &data[1..]);
    }

    /// Columns of 200 and 220 over a base of 150 fit 150 to 220 in the middle 80% of the pane,
    /// 141.25 to 228.75; a provider's 0 to 1,000 fits -125 to 1,125, and the main series' 50 to
    /// 60 fits 48.75 to 61.25.
    /// The price line, the base line of a percentage scale at the first visible close and the
    /// last value label follow the series style.
    #[test]
    fn series_style_draws_columns_and_lines_and_fits_provided_ranges() {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
        let id = chart.add_series(SeriesOptions {
            scale: SeriesScale::Separate(2),
            chart_type: ChartType::Columns,
            color: Color32::MAGENTA,
            up_color: Color32::MAGENTA,
            columns_base: Some(150.0),
            ..Default::default()
        });
        chart.set_data(id, bars(&[200.0, 220.0]));
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
        let range = chart.series_price_scale(id).unwrap().range();
        assert!((range.min - 141.25).abs() < 1e-10 && (range.max - 228.75).abs() < 1e-10);
        let map = chart.series_map(&chart.price_series[0], chart.last_rendered_price_rect);
        let columns: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::Rect(column)
                    if column.fill == Color32::MAGENTA
                        && chart.last_rendered_price_rect.contains_rect(column.rect) =>
                {
                    Some(column.rect.y_range())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            columns,
            [200.0, 220.0]
                .map(|close| { egui::Rangef::new(map.price_to_y(close), map.price_to_y(150.0)) })
        );

        let style = chart.series_style_mut(id).unwrap();
        style.autoscale = Some(crate::model::AutoscaleProvider::new(|_, _| {
            Some((0.0, 1000.0))
        }));
        style.price_line_visible = true;
        style.price_line_color = Some(Color32::GOLD);
        style.price_line_style = LineStyle::Solid;
        style.base_line_visible = true;
        style.last_value_visible = false;
        chart.series_price_scale_mut(id).unwrap().options_mut().mode = PriceScaleMode::Percentage;
        chart.series_style_mut(SeriesId::MAIN).unwrap().autoscale =
            Some(crate::model::AutoscaleProvider::new(|bars, default| {
                assert_eq!(bars.len(), 2);
                assert_eq!(default, Some((100.0, 110.0)));
                Some((50.0, 60.0))
            }));
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
        let map = chart.series_map(&chart.price_series[0], chart.last_rendered_price_rect);
        let range = chart.series_price_scale(id).unwrap().range();
        let fitted = (
            map.transform().to_price(range.min),
            map.transform().to_price(range.max),
        );
        assert!((fitted.0 + 125.0).abs() < 1e-9 && (fitted.1 - 1125.0).abs() < 1e-9);
        let range = chart.state.price_scale().range();
        assert!((range.min - 48.75).abs() < 1e-10 && (range.max - 61.25).abs() < 1e-10);
        for (price, color) in [
            (220.0, Color32::GOLD),
            (200.0, DESIGN_TOKENS.semantic.extended.gray),
        ] {
            assert!(painted.iter().any(|shape| matches!(shape,
                egui::Shape::LineSegment { points, stroke }
                    if stroke.color == color && points[0].y == map.price_to_y(price)
            )));
        }
        assert_eq!(
            chart
                .secondary_axis_values(SeriesScale::Separate(2), chart.last_rendered_price_rect)
                .count(),
            0
        );
    }

    /// Each added series writes its last value with its own scale's decimals, led by its own
    /// name once the axis shows names, and a host formatter still replaces the decimals.
    #[test]
    fn added_series_value_tags_use_their_scale_precision_and_name() {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
        let mut add = |name: &str, key, decimals, last| {
            let id = chart.add_series(SeriesOptions {
                name: name.into(),
                scale: key,
                ..Default::default()
            });
            chart.set_data(id, bars(&[last, last]));
            chart
                .series_price_scale_mut(id)
                .unwrap()
                .set_price_precision(Some(decimals));
            id
        };
        let alpha = add("Alpha", SeriesScale::Separate(2), 3, 200.5);
        add("Beta", SeriesScale::Separate(3), 0, 300.5);
        chart.config.price_axis.show_symbol_name = true;
        frame(&ctx, &mut chart, 0.0, vec![]);
        let labels = |chart: &Chart, key, rect| {
            chart
                .secondary_axis_values(key, rect)
                .map(|(.., label)| label)
                .collect::<Vec<_>>()
        };
        let rect = chart.last_rendered_price_rect;
        assert_eq!(
            labels(&chart, SeriesScale::Separate(2), rect),
            ["Alpha 200.500"]
        );
        assert_eq!(labels(&chart, SeriesScale::Separate(3), rect), ["Beta 300"]);
        chart.config.price_axis.show_symbol_name = false;
        assert_eq!(labels(&chart, SeriesScale::Separate(2), rect), ["200.500"]);
        chart.set_series_formatter(
            alpha,
            Some(std::sync::Arc::new(
                crate::scales::CustomPriceFormatter::new(|price| format!("{price:.1} USD")),
            )),
        );
        assert_eq!(
            labels(&chart, SeriesScale::Separate(2), rect),
            ["200.5 USD"]
        );
    }

    /// Every scale is fitted once across all its visible members; hidden and differently
    /// assigned values cannot stretch it. Labels reserve space only for visible axes.
    #[test]
    fn series_scales_fit_visible_members_and_reserve_their_axes() {
        let ctx = egui::Context::default();
        for key in [
            SeriesScale::Right,
            SeriesScale::Left,
            SeriesScale::Overlay(1),
            SeriesScale::Separate(2),
        ] {
            let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
            let id = chart.add_series(SeriesOptions {
                scale: key,
                color: Color32::MAGENTA,
                ..Default::default()
            });
            chart.set_data(id, bars(&[200.0, 220.0]));
            frame(&ctx, &mut chart, 0.0, vec![]);
            let range = chart.series_price_scale_mut(id).unwrap().range();
            let expected = if key == SeriesScale::Right {
                (85.0, 235.0)
            } else {
                (197.5, 222.5)
            };
            assert!(
                (range.min - expected.0).abs() < 1e-10 && (range.max - expected.1).abs() < 1e-10,
                "{key:?}: {range:?}"
            );
            assert_eq!(
                chart.series_axis_keys().is_empty(),
                matches!(key, SeriesScale::Right | SeriesScale::Overlay(_))
            );
            chart.set_series_visible(id, false);
            frame(&ctx, &mut chart, 0.1, vec![]);
            assert!(chart.series_axis_keys().is_empty());
            let range = chart.state.price_scale().range();
            assert!((range.min - 98.75).abs() < 1e-10 && (range.max - 111.25).abs() < 1e-10);
            // Secondary bars can occupy whitespace beyond the main history.
            let future = bars(&[200.0, 220.0])
                .into_iter()
                .map(|mut bar| {
                    bar.time += TimeDelta::minutes(10);
                    bar
                })
                .collect();
            chart.set_data(id, future);
            chart.series_price_scale_mut(id).unwrap().options_mut().mode =
                PriceScaleMode::Percentage;
            chart.set_series_visible(id, true);
            chart.state.time_scale_mut().set_bar_spacing(100.0);
            chart.state.time_scale_mut().set_right_offset(12.0);
            let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.2, vec![]);
            assert!(chart.state.visible_data().is_empty());
            if key == SeriesScale::Right {
                assert_eq!(
                    chart.last_price_map.transform().mode,
                    PriceScaleMode::Percentage
                );
            }
            assert!(
                painted.iter().any(|shape| matches!(shape,
                    egui::Shape::Path(path)
                        if path.stroke.color == egui::epaint::ColorMode::Solid(Color32::MAGENTA)
                )),
                "secondary line in main whitespace: {key:?}"
            );
        }
        for key in [
            SeriesScale::Right,
            SeriesScale::Left,
            SeriesScale::Separate(5),
            SeriesScale::Separate(8),
        ] {
            let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
            chart.config.show_volume = false;
            let first = chart.add_indicator(Box::new(crate::studies::CustomIndicator::new(
                "Shared low",
                |_| vec![crate::studies::IndicatorValue::Single(200.0); 2],
            )));
            let second = chart.add_indicator(Box::new(crate::studies::CustomIndicator::new(
                "Shared high",
                |_| vec![crate::studies::IndicatorValue::Single(300.0); 2],
            )));
            for id in [first, second] {
                assert!(chart.set_indicator_scale(id, crate::studies::StudyScale::Shared(key)));
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            let mapping = |chart: &Chart, id| {
                let study = chart
                    .indicators()
                    .find(|(study, _)| *study == id)
                    .unwrap()
                    .1;
                crate::chart::renderers::study_mapping(study, &chart.get_chart_mapping()).price
            };
            let one = mapping(&chart, first);
            let two = mapping(&chart, second);
            assert_eq!(one.range(), two.range(), "assigned studies share a range");
            assert!((one.price_to_y(250.0) - two.price_to_y(250.0)).abs() < 0.01);
            assert!(one.range().min <= 200.0 && one.range().max >= 300.0);
            assert_eq!(
                chart.series_axis_keys().len(),
                usize::from(key != SeriesScale::Right)
            );
            if key == SeriesScale::Right {
                assert!(chart.set_indicator_autoscale(second, false));
                frame(&ctx, &mut chart, 0.05, vec![]);
                assert!(mapping(&chart, first).range().max < 300.0);
                assert!(chart.set_indicator_autoscale(second, true));
            }
            // A manually fixed 100..300 scale places 200 halfway down the pane.
            if key == SeriesScale::Right {
                chart.state.price_scale_mut().set_range(100.0, 300.0);
            } else {
                chart
                    .series_scales
                    .get_mut(&key)
                    .unwrap()
                    .set_range(100.0, 300.0);
            }
            frame(&ctx, &mut chart, 0.1, vec![]);
            let map = mapping(&chart, first);
            let rect = chart.last_rendered_price_rect;
            assert!((map.price_to_y(200.0) - rect.center().y).abs() < 0.01);
            if key == SeriesScale::Left {
                chart.series_scales.get_mut(&key).unwrap().reset();
                chart
                    .series_scales
                    .get_mut(&key)
                    .unwrap()
                    .options_mut()
                    .mode = PriceScaleMode::Percentage;
                chart.set_percentage_formatter(Some(std::sync::Arc::new(
                    crate::scales::CustomPriceFormatter::new(|value| format!("p {value:.2}")),
                )));
                let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.15, vec![]);
                assert!(
                    painted.iter().any(|shape| matches!(shape,
                        egui::Shape::Text(text) if text.galley.text().starts_with("p ")
                            && text.pos.x < chart.last_rendered_price_rect.left()
                    )),
                    "shared study ticks use percentage units and the host formatter"
                );
                chart
                    .series_scales
                    .get_mut(&key)
                    .unwrap()
                    .options_mut()
                    .mode = PriceScaleMode::Normal;
            }
            // A group moved away from the main pane must not stretch the main price scale.
            let pane = chart.add_pane();
            assert!(chart.move_series_to_pane(first, pane));
            assert!(chart.move_series_to_pane(second, pane));
            chart.state.price_scale_mut().reset();
            frame(&ctx, &mut chart, 0.2, vec![]);
            assert!(chart.state.price_scale().range().max < 200.0);
            if key == SeriesScale::Left {
                assert!(chart.set_indicator_scale(
                    first,
                    crate::studies::StudyScale::Shared(SeriesScale::Right)
                ));
                chart
                    .pane_layout
                    .panes
                    .iter_mut()
                    .find(|item| item.id == pane)
                    .unwrap()
                    .scale
                    .options_mut()
                    .mode = PriceScaleMode::Logarithmic;
                frame(&ctx, &mut chart, 0.3, vec![]);
                let map = chart.series_price_map(first).unwrap();
                assert!(
                    map.transform().to_price(map.range().max) < 300.0,
                    "a secondary study cannot stretch the logarithmic primary pane scale"
                );
            }
        }
        // Hidden study axes do not consume a secondary scale's reserved gutter.
        for width in [45.0, 240.0, 800.0] {
            let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
            chart.set_series_scale(SeriesId::MAIN, SeriesScale::Overlay(0));
            chart.config.padding = 8.0;
            chart.add_series(SeriesOptions {
                scale: SeriesScale::Left,
                ..Default::default()
            });
            chart.add_indicator(Box::new(
                crate::studies::CustomIndicator::new("Independent", |_| Vec::new())
                    .with_scale(crate::studies::StudyScale::Independent),
            ));
            let widget = Rect::from_min_size(Pos2::new(7.0, 11.0), egui::vec2(width, 400.0));
            let plot = chart.calculate_layout_rects(widget).price_rect;
            let axes = chart.secondary_axes(widget, plot);
            assert_eq!(axes.len(), 1);
            assert_eq!(axes[0].1.left(), widget.left() + 8.0, "{width}");
            assert_eq!(axes[0].1.right(), plot.left(), "{width}");
        }
    }

    /// A candle series moved to its own pane is fitted to its lows and highs on that pane's
    /// scale and drawn there, leaving the main scale alone, and draws in the main pane again
    /// once moved back.
    #[test]
    fn series_move_between_panes_with_their_chart_type_and_scale() {
        let ctx = egui::Context::default();
        let up = Color32::from_rgb(1, 200, 2);
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0, 120.0])));
        let id = chart.add_series(SeriesOptions {
            chart_type: ChartType::Candles,
            up_color: up,
            ..Default::default()
        });
        let candles = bars(&[0.0; 3])
            .into_iter()
            .map(|bar| Bar::new(bar.time, 200.0, 230.0, 190.0, 220.0, 1.0))
            .collect();
        chart.set_data(id, candles);
        assert!(!chart.move_series_to_pane(id, PaneId(99)));
        let pane = chart.add_pane();
        assert!(chart.move_series_to_pane(id, pane));
        assert_eq!(chart.series_pane(id), Some(pane));
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
        let main = chart.last_rendered_price_rect;
        assert!(chart.state.price_scale().range().max < 200.0);
        let range = chart.series_price_scale(id).unwrap().range();
        assert!(
            range.min <= 190.0 && (230.0..300.0).contains(&range.max),
            "{range:?}"
        );
        let map = chart.series_price_map(id).unwrap();
        assert!(map.price_to_y(230.0) > main.bottom());
        assert_eq!(
            chart.format_series_price(id, 230.0).as_deref(),
            Some("230.00")
        );
        // Candle bodies in the plot, not the last-value tag on the axis.
        let bodies = |painted: &[egui::Shape]| -> Vec<Rect> {
            painted
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Rect(body)
                        if body.fill == up && body.rect.right() < main.right() =>
                    {
                        Some(body.rect)
                    }
                    _ => None,
                })
                .collect()
        };
        let drawn = bodies(&painted);
        assert_eq!(drawn.len(), 3);
        assert!(drawn.iter().all(|body| body.top() > main.bottom()));

        #[derive(Clone)]
        struct Host;
        impl crate::scales::PriceFormatter for Host {
            fn format(&self, value: f64) -> String {
                format!("label {value:.2}")
            }
            fn format_tick(&self, value: f64) -> String {
                format!("tick {value:.2}")
            }
            fn clone_box(&self) -> Box<dyn crate::scales::PriceFormatter> {
                Box::new(self.clone())
            }
        }
        chart.set_price_formatter(Some(std::sync::Arc::new(Host)));
        chart.set_percentage_formatter(Some(std::sync::Arc::new(Host)));
        chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Normal;
        for (index, (mode, expected)) in [
            (PriceScaleMode::Normal, "label 230.00"),
            (PriceScaleMode::Logarithmic, "label 230.00"),
            (PriceScaleMode::Percentage, "label 4.55"),
            (PriceScaleMode::IndexedTo100, "label 104.55"),
        ]
        .into_iter()
        .enumerate()
        {
            let time = 0.2 + index as f64 * 0.2;
            chart.series_price_scale_mut(id).unwrap().options_mut().mode = mode;
            frame(&ctx, &mut chart, time, vec![]);
            let map = chart.series_price_map(id).unwrap();
            let pos = Pos2::new(
                chart.get_chart_mapping().rect.center().x,
                map.price_to_y(210.0),
            );
            let painted = crate::widget::test_frames::shapes(
                &ctx,
                &mut chart,
                time + 0.1,
                vec![egui::Event::PointerMoved(pos)],
            );
            assert_eq!(
                chart.format_series_price(id, 230.0).as_deref(),
                Some(expected)
            );
            let labels: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Text(text) if text.pos.y > main.bottom() => {
                        Some(text.galley.text())
                    }
                    _ => None,
                })
                .collect();
            assert!(
                labels.iter().any(|text| text.starts_with("tick ")),
                "{mode:?}: {labels:?}"
            );
            let hover_label = chart.format_series_price(id, 210.0).unwrap();
            assert!(
                labels.contains(&hover_label.as_str()),
                "{mode:?}: {labels:?}"
            );
        }
        chart.series_price_scale_mut(id).unwrap().options_mut().mode = PriceScaleMode::Normal;
        frame(&ctx, &mut chart, 0.94, vec![]);
        assert!(chart.set_series_formatter(
            id,
            Some(std::sync::Arc::new(
                crate::scales::CustomPriceFormatter::new(|price| format!("own {price:.1}")),
            ))
        ));
        assert_eq!(
            chart.format_series_price(id, 230.0).as_deref(),
            Some("own 230.0")
        );
        frame(&ctx, &mut chart, 0.95, vec![]);
        assert!(chart.set_series_formatter(id, None));
        assert_eq!(
            chart.format_series_price(id, 230.0).as_deref(),
            Some("label 230.00")
        );

        assert!(chart.move_series_to_pane(id, PaneId::MAIN));
        assert_eq!(chart.series_pane(id), Some(PaneId::MAIN));
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 1.0, vec![]);
        let main = chart.last_rendered_price_rect;
        let drawn = bodies(&painted);
        assert_eq!(drawn.len(), 3);
        assert!(drawn.iter().all(|body| main.contains_rect(*body)));
        for show_study_values in [false, true] {
            chart.config.price_axis.show_study_values = show_study_values;
            let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 1.1, vec![]);
            assert!(
                painted.iter().any(|shape| matches!(shape,
                    egui::Shape::Text(text) if text.pos.x < chart.last_rendered_price_rect.right()
                        && text.galley.text().contains("label 220.00")
                )),
                "study axis settings must preserve comparison legend values"
            );
        }
    }

    /// Bars can be popped from, and looked up in, any bar series; live changes are reported
    /// once per series in a row; the draw order can be changed.
    #[test]
    fn series_data_is_queried_popped_and_reported() {
        use crate::model::LogicalRange;
        use crate::widget::{BarsInRange, ChartEvent, MismatchDirection};
        let mut chart = Chart::new(BarData::from_bars(bars(&[1.0, 2.0, 3.0, 4.0, 5.0])));
        let id = chart.add_series(SeriesOptions::default());
        // Bars at the main bars 0, 2 and 4.
        let data = bars(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        chart.set_data(id, vec![data[0].clone(), data[2].clone(), data[4].clone()]);
        let minute = |index| data[0].time + TimeDelta::minutes(index);
        for (time, direction, expected) in [
            (minute(2), MismatchDirection::Exact, Some(1)),
            (minute(1), MismatchDirection::Exact, None),
            (minute(1), MismatchDirection::NearestLeft, Some(0)),
            (minute(1), MismatchDirection::NearestRight, Some(1)),
            (minute(-1), MismatchDirection::NearestLeft, None),
            (minute(5), MismatchDirection::NearestRight, None),
        ] {
            assert_eq!(
                chart.bar_index_at(id, time, direction),
                expected,
                "{time} {direction:?}"
            );
        }
        for ((left, right), bars, bars_before, bars_after) in [
            ((1.0, 3.0), 1..2, 1.0, 1.0),
            // Two bars of whitespace before the first bar.
            ((-2.0, 2.4), 0..2, -2.0, 1.0),
            // Five bars of whitespace after the last bar, at 4.
            ((2.6, 9.0), 2..3, 2.0, -5.0),
            ((0.6, 1.4), 1..1, 1.0, 2.0),
        ] {
            assert_eq!(
                chart.bars_in_logical_range(id, LogicalRange::new(left, right)),
                Some(BarsInRange {
                    bars,
                    bars_before,
                    bars_after
                }),
                "{left}..{right}"
            );
        }

        let _ = chart.drain_events().count();
        chart.update(id, data[3].clone());
        chart.update(id, bars(&[f64::NAN]).remove(0));
        chart.update(id, Bar::new(minute(3), 13.0, 15.0, 13.0, 15.0, 1.0));
        assert_eq!(
            chart.pop_bars(id, 2),
            [
                Bar::new(minute(3), 13.0, 15.0, 13.0, 15.0, 1.0),
                data[4].clone()
            ]
        );
        assert_eq!(
            chart.pop_bars(SeriesId::MAIN, 1),
            bars(&[1.0, 2.0, 3.0, 4.0, 5.0])[4..]
        );
        assert!(chart.pop_bars(SeriesId(99), 1).is_empty());
        assert_eq!(chart.bars(id), [data[0].clone(), data[2].clone()]);
        assert_eq!(chart.bars(SeriesId::MAIN).len(), 4);
        assert_eq!(
            chart.drain_events().collect::<Vec<_>>(),
            [
                ChartEvent::DataUpdated {
                    series: id,
                    bars: 2
                },
                ChartEvent::DataUpdated {
                    series: SeriesId::MAIN,
                    bars: 4
                },
            ]
        );

        let other = chart.add_series(SeriesOptions::default());
        assert!(chart.set_series_order(other, 0));
        assert_eq!(
            chart.series().map(|(id, _)| id).collect::<Vec<_>>(),
            [other, id]
        );
        assert_eq!(chart.series_order(id), Some(1));
    }
}
