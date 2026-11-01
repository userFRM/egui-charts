//! Host-fed bar series and their shared price scales.

use crate::chart::rendering;
use crate::data::FeedState;
use crate::model::{
    AreaStyle, Bar, BaselineStyle, ChartType, ChartTypeParams, ChartTypeStyles, LinePlotStyle,
    LineStyle, Marker, MarkerOptions, PriceLine, SeriesScale, SeriesStyle,
};
use crate::scales::StudyFormat;
use crate::scales::{PriceMap, PriceScale, PriceScaleMode};
use crate::studies::{Indicator, IndicatorValues, PlotStyle, PlotType};
use crate::tokens::DESIGN_TOKENS;
use crate::widget::{Chart, ChartElementId, PaneId, SeriesId};
use egui::Color32;
use serde::{Deserialize, Serialize};

/// Display settings of a host-fed bar series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub(super) fn type_params(&self) -> ChartTypeParams {
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
pub(in crate::widget) fn base_line(map: &PriceMap) -> Option<PriceLine> {
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

pub(in crate::widget) struct PriceSeries {
    pub id: SeriesId,
    pub options: SeriesOptions,
    pub visible: bool,
    pub bars: Vec<Bar>,
    /// Changes when bars other than the last one change, as the main bars' revision does.
    pub revision: u64,
    pub heikin_ashi: rendering::candles::HeikinAshiCache,
    pub feed: FeedState,
    pub markers: crate::widget::marks::TimeOrdered<Marker>,
    pub marker_options: MarkerOptions,
    pub change_marks: Option<crate::widget::change_marks::ChangeMarks>,
    pub lines: Vec<(crate::widget::PriceLineId, PriceLine)>,
}

impl PriceSeries {
    /// The prices of `bar` that the series draws, lowest first.
    pub(in crate::widget) fn extremes(&self, bar: &Bar) -> [f64; 2] {
        if self.options.chart_type.uses_ohlc() {
            [bar.low, bar.high]
        } else {
            [bar.close; 2]
        }
    }

    /// The last-value tag colour: the bar's own colour, or its direction for the OHLC types.
    pub(in crate::widget) fn value_color(&self, bar: &Bar) -> Color32 {
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
pub(in crate::widget) struct SeriesLegend {
    name: String,
    style: PlotStyle,
    close: Option<f64>,
    values: IndicatorValues,
    formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    /// Decimals of the series' price scale, when it sets them.
    precision: Option<usize>,
    /// Groups the thousands of built-in prices, from the series' price scale.
    thousands_sep: Option<char>,
}

impl SeriesLegend {
    pub(in crate::widget) fn new(
        item: &PriceSeries,
        close: Option<f64>,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
        precision: Option<usize>,
        thousands_sep: Option<char>,
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
            thousands_sep,
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
            || {
                crate::scales::group_thousands(
                    StudyFormat::Price.format(value, self.precision),
                    self.thousands_sep,
                )
            },
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
            markers: Default::default(),
            marker_options: MarkerOptions::default(),
            change_marks: None,
            lines: Vec::new(),
        });
        self.host_events
            .push(crate::widget::ChartEvent::SeriesAdded(id));
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
                .map(|pane| {
                    pane.price_map()
                        .with_thousands_sep(self.config.thousands_sep)
                });
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
        Some(map.label(
            transform.to_scale(price),
            map.price_precision().unwrap_or(2),
            self.series_label_formatter(id, transform.mode).as_deref(),
        ))
    }
}
